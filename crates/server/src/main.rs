// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Server
//! This is a daemon that manages a VM and containers running inside the VM.
//! It creates and starts a macOS VM using the `vmm` crate on the startup.
//! It listens on a Unix domain socket for incoming requests to manage containers.
//! It forwards the requests to the in-VM agent using the `vmm` crate.

use std::{
    collections::HashMap,
    os::{
        fd::AsRawFd,
        unix::{fs::FileTypeExt, net::UnixStream},
    },
    path::PathBuf,
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;

use libakari::{
    container_rpc::ContainerCommand,
    path::{aux_sock_path, root_path},
    vm_config::{load_vm_config, MacosVmConfig, MacosVmSerial},
    vm_rpc::{self, Response, VmCommand},
};
use log::{debug, info};
use protos::{
    types::empty::Empty,
    vm::{
        vm::{
            ConnectRequest, CreateRequest, DeleteRequest, KillRequest, StartRequest, StateRequest,
        },
        vm_ttrpc_async::VmService,
    },
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use ttrpc::{
    asynchronous::{Server, TtrpcContext},
    Error,
};

#[derive(clap::Parser)]
struct Opts {
    /// root directory to store container state
    #[clap(short, long)]
    pub root: Option<PathBuf>,
    /// Specify the path to the aux socket
    #[clap(short, long)]
    aux_sock: Option<PathBuf>,
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
struct ContainerService {
    state_map: Arc<RwLock<ContainerStateMap>>,
    cmd_tx: mpsc::Sender<VmCommand>,
    data_rx: Arc<RwLock<VsockRx>>,
}

impl ContainerService {
    // Send a command to the in-VM agent to create a container.
    async fn do_create(
        &self,
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

    async fn do_delete(&self, container_id: String) -> Result<(), vm_rpc::Error> {
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

    async fn do_kill(&self, container_id: String) -> Result<(), vm_rpc::Error> {
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

    async fn do_start(&self, container_id: String) -> Result<(), vm_rpc::Error> {
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

    async fn do_state(&self, container_id: String) -> Result<Response, vm_rpc::Error> {
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

    async fn do_connect(&self, container_id: String, _port: u32) -> Result<(), vm_rpc::Error> {
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

#[async_trait]
impl VmService for ContainerService {
    async fn create(&self, _ctx: &TtrpcContext, req: CreateRequest) -> ttrpc::Result<Empty> {
        let args = req.args();
        let bundle = args.bundle().into();
        let rootfs = args.rootfs().into();
        let stdin = args.stdin.as_ref().map(|stdin| stdin.into());
        let stdout = args.stdout.as_ref().map(|stdout| stdout.into());
        let stderr = args.stderr.as_ref().map(|stderr| stderr.into());
        let create_req = vm_rpc::CreateRequest {
            bundle,
            rootfs,
            stdin,
            stdout,
            stderr,
        };
        match self.do_create(req.container_id, create_req).await {
            Ok(_) => Ok(Empty::new()),
            Err(e) => Err(Error::Others(e.to_string())),
        }
    }

    async fn delete(&self, _ctx: &TtrpcContext, req: DeleteRequest) -> ttrpc::Result<Empty> {
        match self.do_delete(req.container_id).await {
            Ok(_) => Ok(Empty::new()),
            Err(e) => Err(Error::Others(e.to_string())),
        }
    }

    async fn kill(&self, _ctx: &TtrpcContext, req: KillRequest) -> ttrpc::Result<Empty> {
        match self.do_kill(req.container_id).await {
            Ok(_) => Ok(Empty::new()),
            Err(e) => Err(Error::Others(e.to_string())),
        }
    }

    async fn start(&self, _ctx: &TtrpcContext, req: StartRequest) -> ttrpc::Result<Empty> {
        match self.do_start(req.container_id).await {
            Ok(_) => Ok(Empty::new()),
            Err(e) => Err(Error::Others(e.to_string())),
        }
    }

    async fn state(&self, _ctx: &TtrpcContext, req: StateRequest) -> ttrpc::Result<Empty> {
        match self.do_state(req.container_id).await {
            Ok(_) => Ok(Empty::new()),
            Err(e) => Err(Error::Others(e.to_string())),
        }
    }

    async fn connect(&self, _ctx: &TtrpcContext, req: ConnectRequest) -> ttrpc::Result<Empty> {
        match self.do_connect(req.container_id, req.port).await {
            Ok(_) => Ok(Empty::new()),
            Err(e) => Err(Error::Others(e.to_string())),
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
            vm_rpc::VmCommand::Stop => vm.kill()?,
            vm_rpc::VmCommand::Pause => todo!("Pause"),
            vm_rpc::VmCommand::Resume => todo!("Resume"),
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let opts = Opts::parse();

    let root_path = root_path(opts.root)?;
    let aux_sock_path = aux_sock_path(&root_path, opts.aux_sock);

    match aux_sock_path.try_exists() {
        Ok(exist) => {
            if exist {
                let metadata = std::fs::metadata(&aux_sock_path)?;
                if metadata.file_type().is_socket() {
                    std::fs::remove_file(&aux_sock_path)?;
                } else {
                    anyhow::bail!("The aux socket path exists and is not a socket");
                }
            }
        }
        Err(e) => {
            anyhow::bail!("Failed to check if the aux socket path exists: {}", e);
        }
    }

    // TODO: Create a socket for the VM management

    let console_path = opts
        .console_sock
        .unwrap_or_else(|| root_path.join("console.sock"));

    let vm_config_path = root_path.join("vm.json");
    let mut vm_config = load_vm_config(&vm_config_path)?;
    vm_config.serial = Some(MacosVmSerial { path: console_path });

    info!("Creating VM from config file: {:?}", vm_config_path);
    let (thread, cmd_tx, data_rx) = create_vm(vm_config).await?;

    let data_rx = Arc::new(RwLock::new(data_rx));

    info!("Starting VM");
    cmd_tx.send(vm_rpc::VmCommand::Start).await?;

    info!("Listening on: {:?}", aux_sock_path);
    let v = Box::new(ContainerService {
        state_map: Arc::new(RwLock::new(HashMap::new())),
        cmd_tx,
        data_rx: data_rx.clone(),
    }) as Box<dyn protos::vm::vm_ttrpc_async::VmService + Sync + Send>;
    let v = Arc::new(v);
    let vservice = protos::vm::vm_ttrpc_async::create_vm_service(v);

    let mut server = Server::new()
        .bind(aux_sock_path.as_path().to_str().unwrap())
        .unwrap()
        .register_service(vservice);

    server.start().await?;

    thread.await??;

    Ok(())
}
