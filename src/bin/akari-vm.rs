// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Virtual Machine
//! This is a daemon that listens for requests from the akari OCI runtime to manage containers.

use std::{
    collections::HashMap,
    future::Future,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{mpsc, Arc, RwLock},
    thread,
};

use anyhow::Result;
use clap::Parser;

use akari::{
    api::{self, Api, Response},
    traits::{ReadFrom, WriteTo},
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
    socket: PathBuf,
}

#[derive(Clone, Debug)]
struct VmState {
    config: vmm::api::MacosVmConfig,
    bundle: PathBuf,
    status: api::VmStatus,
}

type VmStateMap = HashMap<String, VmState>;

#[derive(Clone)]
struct ApiServer {
    state_map: Arc<RwLock<VmStateMap>>,
    threads: Arc<RwLock<Vec<thread::JoinHandle<Result<()>>>>>,
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
        };

        state_map.insert(container_id.clone(), state);

        let (tx, rx) = mpsc::channel::<api::VmStatus>();
        let thread = thread::spawn(move || -> Result<()> {
            let serial_sock = match &vm_config.serial {
                Some(serial) => Some(UnixStream::connect(&serial.path)?),
                None => None,
            };

            let config = vmm::vm::create_vm(vm_config, &serial_sock)?;
            let vm = vmm::vm::Vm::new(config.clone())?;
            tx.send(api::VmStatus::Created)?;

            let cmd_listener = UnixListener::bind("/tmp/cmd.sock")?;
            for stream in cmd_listener.incoming() {
                let mut stream = stream?;
                let cmd = api::Command::recv(&mut stream)?;
                match cmd {
                    api::Command::Start => vm.start()?,
                    api::Command::Kill => vm.kill()?,
                    _ => todo!(),
                }
            }
            Ok(())
        });

        self.threads.write().expect("Lock poisoned").push(thread);

        state_map
            .get_mut(&container_id.clone())
            .ok_or(anyhow::anyhow!("Container not found"))
            .expect("Container not found")
            .status = rx.recv().expect("Failed to receive status");
    }

    async fn delete(self, _context: ::tarpc::context::Context, container_id: String) {
        let mut state_map = self.state_map.write().expect("Lock poisoned");
        let state = state_map
            .get_mut(&container_id)
            .ok_or(anyhow::anyhow!("Container not found"))
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
            .ok_or(anyhow::anyhow!("Container not found"))
            .expect("Container not found");

        let _result = match state.status {
            api::VmStatus::Created | api::VmStatus::Running => {
                let mut cmd_sock = UnixStream::connect("/tmp/cmd.sock").expect("Failed to connect");
                let cmd = api::Command::Kill;
                cmd.send(&mut cmd_sock).expect("Failed to send command");

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
            .ok_or(anyhow::anyhow!("Container not found"))
            .expect("Container not found");

        let _result = match state.status {
            api::VmStatus::Created => {
                let mut cmd_sock = UnixStream::connect("/tmp/cmd.sock").expect("Failed to connect");
                let cmd = api::Command::Start;
                cmd.send(&mut cmd_sock).expect("Failed to send command");

                state.status = api::VmStatus::Running;
                Ok(())
            }
            api::VmStatus::Running => Err(anyhow::anyhow!("Container already running")),
            _ => Err(anyhow::anyhow!("Container not created")),
        };
    }

    async fn state(self, _context: ::tarpc::context::Context, container_id: String) -> Response {
        let state_map = self.state_map.read().expect("Lock poisoned");
        let state = state_map
            .get(&container_id)
            .ok_or(anyhow::anyhow!("Container not found"))
            .expect("Container not found");

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

    let mut listener = serde_transport::unix::listen(opts.socket, Json::default).await?;
    listener.config_mut().max_frame_length(usize::MAX);

    let state_map = Arc::new(RwLock::new(HashMap::new()));

    let threads = Arc::new(RwLock::new(Vec::new()));

    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .map(|channel| {
            let state_map = state_map.clone();
            let threads = threads.clone();
            let server = ApiServer { state_map, threads };
            channel.execute(server.serve()).for_each(spawn)
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}
