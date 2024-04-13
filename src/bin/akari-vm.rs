// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Virtual Machine
//! This is a daemon that listens for requests from the akari OCI runtime to manage containers.

use std::{
    collections::HashMap,
    future::Future,
    os::unix::{fs::FileTypeExt, net::UnixStream},
    path::PathBuf,
    sync::{
        mpsc::{self, Sender},
        Arc, RwLock,
    },
    thread,
};

use anyhow::Result;
use clap::Parser;

use akari::{
    api::{self, Api, Command, Response},
    path::{root_path, vmm_sock_path},
    vmm::{self, api::MacosVmConfig},
};
use futures::{future, stream::StreamExt};
use tarpc::{
    serde_transport,
    server::{self, Channel},
    tokio_serde::formats::Json,
};

#[derive(clap::Parser)]
struct Opts {
    /// root directory to store container state
    #[clap(short, long)]
    pub root: Option<PathBuf>,
    /// Specify the path to the VMM socket
    #[clap(short, long)]
    vmm_sock: Option<PathBuf>,
}

type VmThread = thread::JoinHandle<Result<()>>;
type VmThreadTx = Sender<Command>;

#[derive(Debug)]
struct VmState {
    config: vmm::api::MacosVmConfig,
    bundle: PathBuf,
    status: api::VmStatus,

    thread: Option<VmThread>,
    tx: Option<VmThreadTx>,
}

type VmStateMap = HashMap<String, VmState>;

#[derive(Clone)]
struct ApiServer {
    state_map: Arc<RwLock<VmStateMap>>,
}

impl Api for ApiServer {
    async fn create(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
        vm_config: MacosVmConfig,
        bundle: PathBuf,
    ) {
        let mut state_map = self.state_map.write().expect("Lock poisoned");

        if state_map.contains_key(&container_id) {
            panic!("Container already exists");
        }

        let state = VmState {
            config: vm_config.clone(),
            bundle,
            status: api::VmStatus::Creating,
            thread: None,
            tx: None,
        };

        state_map.insert(container_id.clone(), state);

        let (tx, rx) = mpsc::channel::<api::VmStatus>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<api::Command>();
        let thread = thread::spawn(move || -> Result<()> {
            let serial_sock = match &vm_config.serial {
                Some(serial) => Some(UnixStream::connect(&serial.path)?),
                None => None,
            };

            let config = vmm::vm::create_vm(vm_config, &serial_sock)?;
            let vm = vmm::vm::Vm::new(config.clone())?;
            tx.send(api::VmStatus::Created)?;

            loop {
                let cmd = cmd_rx.recv()?;
                match cmd {
                    api::Command::Start => vm.start()?,
                    api::Command::Kill => vm.kill()?,
                    _ => break, // TODO
                }
            }
            Ok(())
        });

        if let Ok(status) = rx.recv() {
            let state = state_map
                .get_mut(&container_id.clone())
                .expect("Container not found");
            state.status = status;
            state.thread = Some(thread);
            state.tx = Some(cmd_tx);
        }
    }

    async fn delete(self, _context: ::tarpc::context::Context, container_id: String) {
        let mut state_map = self.state_map.write().expect("Lock poisoned");
        let state = state_map
            .get_mut(&container_id)
            .expect("Container not found");

        let _result = match state.status {
            api::VmStatus::Created | api::VmStatus::Stopped => {
                state_map.remove(&container_id);
                Ok(())
            }
            api::VmStatus::Creating => Err(anyhow::anyhow!("Container still creating")),
            api::VmStatus::Running => Err(anyhow::anyhow!("Container still running")),
        };
    }

    async fn kill(self, _context: ::tarpc::context::Context, container_id: String) {
        let mut state_map = self.state_map.write().expect("Lock poisoned");
        let state = state_map
            .get_mut(&container_id)
            .expect("Container not found");

        let _result = match state.status {
            api::VmStatus::Created | api::VmStatus::Running => {
                state
                    .tx
                    .as_ref()
                    .expect("Thread not found")
                    .send(api::Command::Kill)
                    .expect("Failed to send kill command");
                state.status = api::VmStatus::Stopped;
                Ok(())
            }
            api::VmStatus::Stopped => Err(anyhow::anyhow!("Container already stopped")),
            _ => Err(anyhow::anyhow!("Container not created")),
        };
    }

    async fn start(self, _context: ::tarpc::context::Context, container_id: String) {
        let mut state_map = self.state_map.write().expect("Lock poisoned");
        let state = state_map
            .get_mut(&container_id)
            .expect("Container not found");

        let _result = match state.status {
            api::VmStatus::Created => {
                state
                    .tx
                    .as_ref()
                    .expect("Thread not found")
                    .send(api::Command::Start)
                    .expect("Failed to send kill command");
                state.status = api::VmStatus::Running;
                Ok(())
            }
            api::VmStatus::Running => Err(anyhow::anyhow!("Container already running")),
            _ => Err(anyhow::anyhow!("Container not created")),
        };
    }

    async fn state(self, _context: ::tarpc::context::Context, container_id: String) -> Response {
        let state_map = self.state_map.read().expect("Lock poisoned");
        let state = state_map.get(&container_id).expect("Container not found");

        api::Response {
            container_id,
            status: state.status.clone(),
            pid: None,
            config: state.config.clone(),
            bundle: state.bundle.clone(),
        }
    }
}

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

#[tokio::main]

async fn main() -> Result<()> {
    let opts = Opts::parse();

    let root_path = root_path(opts.root)?;
    let vmm_sock_path = vmm_sock_path(&root_path, opts.vmm_sock);

    match vmm_sock_path.try_exists() {
        Ok(exist) => {
            if exist {
                let metadata = std::fs::metadata(&vmm_sock_path)?;
                if metadata.file_type().is_socket() {
                    std::fs::remove_file(&vmm_sock_path)?;
                } else {
                    anyhow::bail!("VMM socket path exists and is not a socket");
                }
            }
        }
        Err(e) => {
            anyhow::bail!("Failed to check if VMM socket path exists: {}", e);
        }
    }

    let mut listener = serde_transport::unix::listen(vmm_sock_path, Json::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);

    let state_map = Arc::new(RwLock::new(HashMap::new()));

    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .map(|channel| {
            let state_map = state_map.clone();
            let server = ApiServer { state_map };
            channel.execute(server.serve()).for_each(spawn)
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}
