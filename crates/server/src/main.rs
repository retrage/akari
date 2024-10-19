// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Server
//!
//! This is a daemon that manages a VM and the sockets connected to the agent.
//! 1. Create a macOS guest VM that an agent runs inside.
//! 2. Listen on a Unix domain socket (`aux.sock`) that accepts ttrpc containerd shim v2 requests.
//! 3. Forward the requests to the agent via the vsock, with some exceptions:
//!   - When creating a container, the server does the following:
//!     - Create a symbolic link of the container rootfs in the shared directory.
//!     - Modify the `config.json` file to use the shared directory.
//!     - Send a request to the agent.
//!     - Wait for the agent to finish creating the container.
//!         - The agent creates a listener socket for the container when it finishes creating the container.
//!     - Connect to the listener socket and expose it as a Unix domain socket.
//! 4. Forward the responses from the agent to the containerd shim v2 requests.

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
use containerd_shim::{
    api::{
        ConnectRequest, ConnectResponse, CreateTaskRequest, CreateTaskResponse, DeleteRequest,
        Empty, KillRequest, StartRequest, StartResponse, StateRequest, StateResponse,
    },
    Context, DeleteResponse, Task as ShimTask, TtrpcContext, TtrpcResult,
};
use containerd_shim_protos::shim_async::{create_task, TaskClient};
use libakari::{
    path::{aux_sock_path, root_path},
    vm_config::{load_vm_config, MacosVmConfig, MacosVmSerial},
    vm_rpc::{self, VmCommand},
};
use log::{debug, error, info};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, RwLock},
    task::JoinHandle,
};
use ttrpc::asynchronous::{Client, Server};

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
    vsock_port: u32,
    vsock_path: PathBuf,
}

type ContainerStateMap = HashMap<String, ContainerState>;

#[derive(Clone)]
struct ContainerService {
    state_map: Arc<RwLock<ContainerStateMap>>,
    cmd_tx: mpsc::Sender<VmCommand>,
}

// Forwards the requests from the client or containerd shim v2 to the unix domain socket connected to the agent.
#[async_trait]
impl ShimTask for ContainerService {
    async fn connect(
        &self,
        _ctx: &TtrpcContext,
        req: ConnectRequest,
    ) -> TtrpcResult<ConnectResponse> {
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client = TaskClient::new(Client::connect(state.vsock_path.to_str().unwrap()).unwrap());
        let res = client.connect(Context::default(), &req).await?;
        Ok(res)
    }

    async fn create(
        &self,
        _ctx: &TtrpcContext,
        req: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        let mut state_map = self.state_map.write().await;

        if state_map.contains_key(req.id()) {
            return Err(ttrpc::Error::Others("Container already exists".to_string()));
        }

        // TODO: Create a symbolic link of the container rootfs in the shared directory.
        // TODO: Modify the `config.json` file to use the shared directory.

        let bundle = PathBuf::from(req.bundle());

        // Create a unique vsock port for the container.
        // Find the smallest used vsock port
        const DEFAULT_MIN_PORT: u32 = 1234;
        let mut vsock_port = DEFAULT_MIN_PORT - 1;
        state_map.values().for_each(|state| {
            vsock_port = std::cmp::max(vsock_port, state.vsock_port);
        });
        vsock_port += 1;

        // TODO: Use root_path
        let vsock_path = PathBuf::from(format!("/tmp/akari_vsock_{}", vsock_port));

        self.cmd_tx
            .send(VmCommand::Connect(vsock_port, vsock_path.clone()))
            .await
            .unwrap();

        let client =
            TaskClient::new(Client::connect(vsock_path.clone().to_str().unwrap()).unwrap());
        let res = client.create(Context::default(), &req).await?;

        let state = ContainerState {
            bundle,
            vsock_port,
            vsock_path,
        };
        state_map.insert(req.id().to_string(), state);

        Ok(res)
    }

    async fn delete(&self, _ctx: &TtrpcContext, req: DeleteRequest) -> TtrpcResult<DeleteResponse> {
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client = TaskClient::new(Client::connect(state.vsock_path.to_str().unwrap()).unwrap());
        let res = client.delete(Context::default(), &req).await?;
        match state.bundle.try_exists() {
            Ok(exist) => {
                if exist
                    && state
                        .bundle
                        .symlink_metadata()
                        .unwrap()
                        .file_type()
                        .is_symlink()
                {
                    std::fs::remove_dir_all(&state.bundle).unwrap(); // TODO
                } else {
                    return Err(ttrpc::Error::Others("Bundle does not exist".to_string()));
                }
            }
            Err(e) => {
                return Err(ttrpc::Error::Others(format!(
                    "Failed to check if the bundle exists: {}",
                    e
                )));
            }
        }
        state_map.remove(req.id());
        Ok(res)
    }

    async fn kill(&self, _ctx: &TtrpcContext, req: KillRequest) -> TtrpcResult<Empty> {
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client = TaskClient::new(Client::connect(state.vsock_path.to_str().unwrap()).unwrap());
        let res = client.kill(Context::default(), &req).await?;
        Ok(res)
    }

    async fn start(&self, _ctx: &TtrpcContext, req: StartRequest) -> TtrpcResult<StartResponse> {
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client = TaskClient::new(Client::connect(state.vsock_path.to_str().unwrap()).unwrap());
        let res = client.start(Context::default(), &req).await?;
        Ok(res)
    }

    async fn state(&self, _ctx: &TtrpcContext, req: StateRequest) -> TtrpcResult<StateResponse> {
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client = TaskClient::new(Client::connect(state.vsock_path.to_str().unwrap()).unwrap());
        let res = client.state(Context::default(), &req).await?;
        Ok(res)
    }
}

async fn handle_cmd(vm: &mut vmm::vm::Vm, cmd_rx: &mut mpsc::Receiver<VmCommand>) -> Result<()> {
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
        vm_rpc::VmCommand::Connect(port, path) => vm.connect(port, &path)?,
        _ => todo!(),
    }
    Ok(())
}

fn vm_thread(vm_config: MacosVmConfig, cmd_rx: &mut mpsc::Receiver<VmCommand>) -> Result<()> {
    let serial_sock = match &vm_config.serial {
        Some(serial) => Some(UnixStream::connect(&serial.path)?),
        None => None,
    };

    let config = vmm::config::Config::from_vm_config(vm_config)?
        .console(serial_sock.as_ref().map(|s| s.as_raw_fd()))?
        .build();
    let mut vm = vmm::vm::Vm::new(config)?;

    let rt = Runtime::new().expect("Failed to create a runtime.");
    rt.block_on(async {
        loop {
            if let Err(e) = handle_cmd(&mut vm, cmd_rx).await {
                error!("Failed to handle command: {}", e);
                break;
            }
        }
    });

    Ok(())
}

async fn create_vm(
    vm_config: MacosVmConfig,
) -> Result<(
    JoinHandle<Result<(), anyhow::Error>>,
    mpsc::Sender<VmCommand>,
)> {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<vm_rpc::VmCommand>(8);

    let thread = tokio::spawn(async move { vm_thread(vm_config, &mut cmd_rx) });

    Ok((thread, cmd_tx))
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

    let console_path = opts
        .console_sock
        .unwrap_or_else(|| root_path.join("console.sock"));

    let vm_config_path = root_path.join("vm.json");
    let mut vm_config = load_vm_config(&vm_config_path)?;
    vm_config.serial = Some(MacosVmSerial { path: console_path });

    info!("Creating VM from config file: {:?}", vm_config_path);
    let (thread, cmd_tx) = create_vm(vm_config).await?;

    info!("Starting VM");
    cmd_tx.send(vm_rpc::VmCommand::Start).await?;

    info!("Listening on: {:?}", aux_sock_path);
    let v = Box::new(ContainerService {
        state_map: Arc::new(RwLock::new(HashMap::new())),
        cmd_tx,
    }) as Box<dyn ShimTask + Sync + Send>;
    let vservice = create_task(v.into());

    let mut server = Server::new()
        .bind(aux_sock_path.as_path().to_str().unwrap())
        .unwrap()
        .register_service(vservice);

    server.start().await?;

    thread.await??;

    Ok(())
}
