// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Virtual Machine
//! This is a daemon that listens for requests from the akari OCI runtime to manage containers.

use std::{
    collections::HashMap,
    future::Future,
    io::Write,
    os::{
        fd::AsRawFd,
        unix::{fs::FileTypeExt, net::UnixStream},
    },
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
use log::{debug, error, info};
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
    ) -> Result<(), api::Error> {
        info!("create: container_id={}, bundle={:?}", container_id, bundle);

        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| api::Error::LockPoisoned)?;

        if state_map.contains_key(&container_id) {
            return Err(api::Error::ContainerAlreadyExists);
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

            let config = vmm::config::Config::from_vm_config(vm_config)?
                .console(serial_sock.as_ref().map(|s| s.as_raw_fd()))?
                .build();
            let vm = vmm::vm::Vm::new(config)?;
            tx.send(api::VmStatus::Created)?;

            let vsock_handler = |stream: &mut UnixStream| {
                stream.write_fmt(format_args!("Hello, world!")).unwrap();
            };

            loop {
                debug!("Waiting for command...");
                let cmd = cmd_rx.recv()?;
                match cmd {
                    api::Command::Start => vm.start()?,
                    api::Command::Kill => vm.kill()?,
                    api::Command::Connect(port) => vm.connect(port, vsock_handler)?,
                    _ => {
                        error!("Unexpected command: {:?}", cmd);
                        break;
                    }
                }
            }
            Ok(())
        });

        if let Ok(status) = rx.recv() {
            let state = state_map
                .get_mut(&container_id.clone())
                .ok_or(api::Error::ContainerNotFound)?;
            state.status = status;
            state.thread = Some(thread);
            state.tx = Some(cmd_tx);
        }

        Ok(())
    }

    async fn delete(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<(), api::Error> {
        info!("delete: container_id={}", container_id);

        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| api::Error::LockPoisoned)?;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(api::Error::ContainerNotFound)?;

        match state.status {
            api::VmStatus::Created | api::VmStatus::Stopped => {
                state_map.remove(&container_id);
                Ok(())
            }
            _ => Err(api::Error::UnpextectedContainerStatus(state.status.clone())),
        }
    }

    async fn kill(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<(), api::Error> {
        info!("kill: container_id={}", container_id);

        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| api::Error::LockPoisoned)?;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(api::Error::ContainerNotFound)?;

        match state.status {
            api::VmStatus::Created | api::VmStatus::Running => {
                state
                    .tx
                    .as_ref()
                    .ok_or(api::Error::ThreadNotFound)?
                    .send(api::Command::Kill)
                    .map_err(|_| api::Error::VmCommandFailed)?;
                state.status = api::VmStatus::Stopped;
                Ok(())
            }
            _ => Err(api::Error::UnpextectedContainerStatus(state.status.clone())),
        }
    }

    async fn start(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<(), api::Error> {
        info!("start: container_id={}", container_id);

        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| api::Error::LockPoisoned)?;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(api::Error::ContainerNotFound)?;

        match state.status {
            api::VmStatus::Created => {
                state
                    .tx
                    .as_ref()
                    .ok_or(api::Error::ThreadNotFound)?
                    .send(api::Command::Start)
                    .map_err(|_| api::Error::VmCommandFailed)?;
                state.status = api::VmStatus::Running;
                Ok(())
            }
            _ => Err(api::Error::UnpextectedContainerStatus(state.status.clone())),
        }
    }

    async fn state(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<Response, api::Error> {
        info!("state: container_id={}", container_id);

        let state_map = self
            .state_map
            .read()
            .map_err(|_| api::Error::LockPoisoned)?;
        let state = state_map
            .get(&container_id)
            .ok_or(api::Error::ContainerNotFound)?;

        let response = api::Response {
            container_id,
            status: state.status.clone(),
            pid: None,
            config: state.config.clone(),
            bundle: state.bundle.clone(),
        };
        Ok(response)
    }

    async fn connect(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
        port: u32,
    ) -> Result<(), api::Error> {
        info!("connect: container_id={}", container_id);

        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| api::Error::LockPoisoned)?;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(api::Error::ContainerNotFound)?;

        match state.status {
            api::VmStatus::Running => {
                state
                    .tx
                    .as_ref()
                    .ok_or(api::Error::ThreadNotFound)?
                    .send(api::Command::Connect(port))
                    .map_err(|_| api::Error::VmCommandFailed)?;
                Ok(())
            }
            _ => Err(api::Error::UnpextectedContainerStatus(state.status.clone())),
        }
    }
}

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

#[tokio::main]

async fn main() -> Result<()> {
    env_logger::init();

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

    info!("Listening on: {:?}", vmm_sock_path);
    let mut listener = serde_transport::unix::listen(vmm_sock_path, Json::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);

    let state_map = Arc::new(RwLock::new(HashMap::new()));

    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .map(|channel| {
            debug!("Accepted connection");
            let state_map = state_map.clone();
            let server = ApiServer { state_map };
            channel.execute(server.serve()).for_each(spawn)
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}
