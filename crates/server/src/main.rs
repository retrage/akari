// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Virtual Machine
//! This is a daemon that listens for requests from the akari OCI runtime to manage containers.

use std::{
    collections::HashMap,
    future::Future,
    os::{
        fd::AsRawFd,
        unix::{fs::FileTypeExt, net::UnixStream},
    },
    path::PathBuf,
    sync::Arc,
};

use anyhow::Result;
use clap::Parser;

use futures::{future, stream::StreamExt};
use libakari::{
    container_rpc::ContainerCommand,
    path::{root_path, vmm_sock_path},
    vm_config::{load_vm_config, MacosVmConfig, MacosVmSerial},
    vm_rpc::{self, Response, VmCommand, VmRpc},
};
use log::{debug, info};
use tarpc::{
    serde_transport,
    server::{self, Channel},
    tokio_serde::formats::Json,
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, RwLock},
    task::JoinHandle,
};

#[derive(clap::Parser)]
struct Opts {
    /// root directory to store container state
    #[clap(short, long)]
    pub root: Option<PathBuf>,
    /// Specify the path to the VMM socket
    #[clap(short, long)]
    vmm_sock: Option<PathBuf>,
    /// Specify the path to the VM console socket
    #[clap(short, long)]
    console_sock: Option<PathBuf>,
}

#[derive(Debug)]
struct ContainerState {
    bundle: PathBuf,
    status: vm_rpc::VmStatus, // TODO: Use ContainerStatus
    vsock_port: u32,
}

type ContainerStateMap = HashMap<String, ContainerState>;
type VsockRx = mpsc::Receiver<(u32, Vec<u8>)>;

#[derive(Clone)]
struct ApiServer {
    state_map: Arc<RwLock<ContainerStateMap>>,
    cmd_tx: mpsc::Sender<VmCommand>,
    data_rx: Arc<RwLock<VsockRx>>,
}

impl VmRpc for ApiServer {
    async fn create(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
        req: vm_rpc::CreateRequest,
    ) -> Result<(), vm_rpc::Error> {
        info!(
            "create: container_id={}, bundle={:?}",
            container_id, req.bundle
        );

        let mut state_map = self.state_map.write().await;

        if state_map.contains_key(&container_id) {
            return Err(vm_rpc::Error::ContainerAlreadyExists);
        }

        let config_path = req.bundle.join("config.json");
        let config = std::fs::read_to_string(&config_path).unwrap(); // TODO
        let config = oci_spec::runtime::Spec::load(config).unwrap(); // TODO

        // Find the smallest used vsock port
        const DEFAULT_MIN_PORT: u32 = 1234;
        let mut port = DEFAULT_MIN_PORT - 1;
        state_map.values().for_each(|state| {
            port = std::cmp::max(port, state.vsock_port);
        });
        port += 1;

        self.cmd_tx
            .send(VmCommand::Connect(port))
            .await
            .map_err(|_| vm_rpc::Error::VmCommandFailed)?;

        let cmd = ContainerCommand::Create(Box::new(config));
        let msg = serde_json::to_string(&cmd).unwrap().as_bytes().to_vec();

        self.cmd_tx
            .send(VmCommand::VsockSend(port, msg))
            .await
            .map_err(|_| vm_rpc::Error::VmCommandFailed)?;
        let mut data_rx = self.data_rx.write().await;
        let (port, _data) = data_rx.recv().await.unwrap();

        let state = ContainerState {
            bundle: req.bundle.clone(),
            status: vm_rpc::VmStatus::Creating,
            vsock_port: port,
        };

        state_map.insert(container_id.clone(), state);

        Ok(())
    }

    async fn delete(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<(), vm_rpc::Error> {
        info!("delete: container_id={}", container_id);

        let mut state_map = self.state_map.write().await;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(vm_rpc::Error::ContainerNotFound)?;

        match state.status {
            vm_rpc::VmStatus::Created | vm_rpc::VmStatus::Stopped => {
                let cmd = ContainerCommand::Delete;
                let msg = serde_json::to_string(&cmd).unwrap().as_bytes().to_vec();
                self.cmd_tx
                    .send(VmCommand::VsockSend(state.vsock_port, msg))
                    .await
                    .map_err(|_| vm_rpc::Error::VmCommandFailed)?;
                self.cmd_tx
                    .send(VmCommand::Disconnect(state.vsock_port))
                    .await
                    .map_err(|_| vm_rpc::Error::VmCommandFailed)?;
                state_map.remove(&container_id);
                Ok(())
            }
            _ => Err(vm_rpc::Error::UnpextectedContainerStatus(
                state.status.clone(),
            )),
        }
    }

    async fn kill(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<(), vm_rpc::Error> {
        info!("kill: container_id={}", container_id);

        let mut state_map = self.state_map.write().await;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(vm_rpc::Error::ContainerNotFound)?;

        match state.status {
            vm_rpc::VmStatus::Created | vm_rpc::VmStatus::Running => {
                let cmd = ContainerCommand::Kill;
                let msg = serde_json::to_string(&cmd).unwrap().as_bytes().to_vec();
                self.cmd_tx
                    .send(VmCommand::VsockSend(state.vsock_port, msg))
                    .await
                    .map_err(|_| vm_rpc::Error::VmCommandFailed)?;
                state.status = vm_rpc::VmStatus::Stopped;
                Ok(())
            }
            _ => Err(vm_rpc::Error::UnpextectedContainerStatus(
                state.status.clone(),
            )),
        }
    }

    async fn start(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<(), vm_rpc::Error> {
        info!("start: container_id={}", container_id);

        let mut state_map = self.state_map.write().await;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(vm_rpc::Error::ContainerNotFound)?;

        match state.status {
            vm_rpc::VmStatus::Created => {
                let cmd = ContainerCommand::Start;
                let msg = serde_json::to_string(&cmd).unwrap().as_bytes().to_vec();
                self.cmd_tx
                    .send(VmCommand::VsockSend(state.vsock_port, msg))
                    .await
                    .map_err(|_| vm_rpc::Error::VmCommandFailed)?;
                state.status = vm_rpc::VmStatus::Running;
                Ok(())
            }
            _ => Err(vm_rpc::Error::UnpextectedContainerStatus(
                state.status.clone(),
            )),
        }
    }

    async fn state(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
    ) -> Result<Response, vm_rpc::Error> {
        info!("state: container_id={}", container_id);

        let state_map = self.state_map.read().await;
        let state = state_map
            .get(&container_id)
            .ok_or(vm_rpc::Error::ContainerNotFound)?;

        let cmd = ContainerCommand::State;
        let msg = serde_json::to_string(&cmd).unwrap().as_bytes().to_vec();
        self.cmd_tx
            .send(VmCommand::VsockSend(state.vsock_port, msg))
            .await
            .map_err(|_| vm_rpc::Error::VmCommandFailed)?;

        // TODO: Get the actual PID
        let response = vm_rpc::Response {
            container_id,
            status: state.status.clone(),
            pid: None,
            bundle: state.bundle.clone(),
        };
        Ok(response)
    }

    async fn connect(
        self,
        _context: ::tarpc::context::Context,
        container_id: String,
        _port: u32,
    ) -> Result<(), vm_rpc::Error> {
        info!("connect: container_id={}", container_id);

        let mut state_map = self.state_map.write().await;
        let state = state_map
            .get_mut(&container_id)
            .ok_or(vm_rpc::Error::ContainerNotFound)?;

        match state.status {
            vm_rpc::VmStatus::Running => {
                // TODO: Implement the container connect process
                Ok(())
            }
            _ => Err(vm_rpc::Error::UnpextectedContainerStatus(
                state.status.clone(),
            )),
        }
    }
}

async fn handle_cmd(
    vm: &mut vmm::vm::Vm,
    cmd_rx: &mut mpsc::Receiver<VmCommand>,
    data_tx: &mut mpsc::Sender<(u32, Vec<u8>)>,
) -> Result<()> {
    loop {
        debug!("Waiting for command...");
        let cmd = cmd_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Command channel closed"))?;
        match cmd {
            vm_rpc::VmCommand::Start => vm.start()?,
            vm_rpc::VmCommand::Kill => vm.kill()?,
            vm_rpc::VmCommand::Connect(port) => vm.connect(port)?,
            vm_rpc::VmCommand::Disconnect(port) => vm.disconnect(port)?,
            vm_rpc::VmCommand::VsockSend(port, data) => vm.vsock_send(port, data)?,
            vm_rpc::VmCommand::VsockRecv(port) => {
                let mut data = Vec::new();
                vm.vsock_recv(port, &mut data)?;
                data_tx.send((port, data)).await?;
            }
        }
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn vm_thread(
    vm_config: MacosVmConfig,
    cmd_rx: &mut mpsc::Receiver<VmCommand>,
    data_tx: &mut mpsc::Sender<(u32, Vec<u8>)>,
) -> Result<()> {
    let serial_sock = match &vm_config.serial {
        Some(serial) => Some(UnixStream::connect(&serial.path)?),
        None => None,
    };

    let config = vmm::config::Config::from_vm_config(vm_config)?
        .console(serial_sock.as_ref().map(|s| s.as_raw_fd()))?
        .build();
    let mut vm = vmm::vm::Vm::new(config)?;

    let rt = Runtime::new().expect("Failed to create a runtime.");
    rt.block_on(handle_cmd(&mut vm, cmd_rx, data_tx))
        .unwrap_or_else(|e| panic!("{}", e));

    Ok(())
}

async fn create_vm(
    vm_config: MacosVmConfig,
) -> Result<(
    JoinHandle<Result<(), anyhow::Error>>,
    mpsc::Sender<VmCommand>,
    mpsc::Receiver<(u32, Vec<u8>)>,
)> {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<vm_rpc::VmCommand>(8);
    let (mut data_tx, data_rx) = mpsc::channel::<(u32, Vec<u8>)>(8);

    let thread = tokio::spawn(async move { vm_thread(vm_config, &mut cmd_rx, &mut data_tx) });

    Ok((thread, cmd_tx, data_rx))
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

    let console_path = opts
        .console_sock
        .unwrap_or_else(|| root_path.join("console.sock"));

    let vm_config_path = root_path.join("vm.json");
    let mut vm_config = load_vm_config(&vm_config_path)?;
    vm_config.serial = Some(MacosVmSerial { path: console_path });

    let (thread, cmd_tx, data_rx) = create_vm(vm_config).await?;
    info!("VM thread created");

    let data_rx = Arc::new(RwLock::new(data_rx));

    info!("Starting VM");
    cmd_tx.send(vm_rpc::VmCommand::Start).await?;

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
            let cmd_tx = cmd_tx.clone();
            let data_rx = data_rx.clone();
            let server = ApiServer {
                state_map,
                cmd_tx,
                data_rx,
            };
            channel.execute(server.serve()).for_each(spawn)
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    thread.await??;

    Ok(())
}
