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
    sync::mpsc,
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
use containerd_shim_protos::{
    protobuf::well_known_types::any::Any,
    shim_async::{create_task, TaskClient},
};
use libakari::{
    agent::{DEFAULT_AGENT_PORT, DEFAULT_MIN_VSOCK_PORT},
    path::{aux_sock_path, root_path},
    vm_config::{load_vm_config, MacosVmConfig, MacosVmSerial, MacosVmSharedDirectory},
    vm_rpc::{self, VmCommand},
};
use log::{debug, error, info};
use tokio::{sync::RwLock, task::JoinHandle, time::sleep};
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
        debug!("Connect request: {:?}", req);
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client =
            TaskClient::new(Client::connect(&self.vsock_path_str(state.vsock_port)).unwrap());
        let res = client.connect(Context::default(), &req).await?;
        Ok(res)
    }

    async fn create(
        &self,
        _ctx: &TtrpcContext,
        req: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        debug!("Create request: {:?}", req);
        let mut state_map = self.state_map.write().await;

        if state_map.contains_key(req.id()) {
            return Err(ttrpc::Error::Others("Container already exists".to_string()));
        }

        // TODO: Create a symbolic link of the container rootfs in the shared directory.
        // TODO: Modify the `config.json` file to use the shared directory.

        let bundle = PathBuf::from(req.bundle());

        let vsock_port = self.allocate_vsock_port().await;
        let mut req = req.clone();
        // req.set_options()
        let options = Any {
            type_url: "akari.io/vsock_port".to_string(),
            value: vsock_port.to_le_bytes().to_vec(),
            ..Default::default()
        };
        req.set_options(options);

        let client = self.vsock_connect_client(DEFAULT_AGENT_PORT).await.unwrap();
        let res = client.create(Context::default(), &req).await?;

        let state = ContainerState { bundle, vsock_port };
        state_map.insert(req.id().to_string(), state);

        Ok(res)
    }

    async fn delete(&self, _ctx: &TtrpcContext, req: DeleteRequest) -> TtrpcResult<DeleteResponse> {
        debug!("Delete request: {:?}", req);
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client =
            TaskClient::new(Client::connect(&self.vsock_path_str(state.vsock_port)).unwrap());
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
        debug!("Kill request: {:?}", req);
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client =
            TaskClient::new(Client::connect(&self.vsock_path_str(state.vsock_port)).unwrap());
        let res = client.kill(Context::default(), &req).await?;
        Ok(res)
    }

    async fn start(&self, _ctx: &TtrpcContext, req: StartRequest) -> TtrpcResult<StartResponse> {
        debug!("Start request: {:?}", req);
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client =
            TaskClient::new(Client::connect(&self.vsock_path_str(state.vsock_port)).unwrap());
        let res = client.start(Context::default(), &req).await?;
        Ok(res)
    }

    async fn state(&self, _ctx: &TtrpcContext, req: StateRequest) -> TtrpcResult<StateResponse> {
        debug!("State request: {:?}", req);
        let mut state_map = self.state_map.write().await;
        let state = state_map.get_mut(req.id()).unwrap(); // TODO
        let client =
            TaskClient::new(Client::connect(&self.vsock_path_str(state.vsock_port)).unwrap());
        let res = client.state(Context::default(), &req).await?;
        Ok(res)
    }
}

impl ContainerService {
    fn vsock_path_path_buf(&self, vsock_port: u32) -> PathBuf {
        // TODO: Use root_path
        PathBuf::from(format!("/tmp/akari_vsock_{}", vsock_port))
    }

    fn vsock_path_str(&self, vsock_port: u32) -> String {
        format!(
            "unix://{}",
            self.vsock_path_path_buf(vsock_port).to_str().unwrap()
        )
    }

    async fn allocate_vsock_port(&self) -> u32 {
        let state_map = self.state_map.read().await;
        let mut vsock_port = DEFAULT_MIN_VSOCK_PORT;
        state_map.values().for_each(|state| {
            vsock_port = std::cmp::max(vsock_port, state.vsock_port);
        });
        vsock_port + 1
    }

    async fn vsock_connect_client(&self, vsock_port: u32) -> Result<TaskClient> {
        let vsock_path = self.vsock_path_path_buf(vsock_port);
        match vsock_path.try_exists() {
            Ok(exist) => {
                if exist {
                    std::fs::remove_file(&vsock_path).unwrap(); // TODO
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!(format!(
                    "Failed to check if the vsock path exists: {}",
                    e
                )));
            }
        }

        self.cmd_tx
            .send(VmCommand::Connect(vsock_port, vsock_path.clone()))
            .unwrap();

        sleep(std::time::Duration::from_secs(5)).await;

        let vsock_path = self.vsock_path_str(vsock_port);
        let client = TaskClient::new(Client::connect(&vsock_path).unwrap());
        Ok(client)
    }
}

fn handle_cmd(vm: &mut vmm::vm::Vm, cmd_rx: &mut mpsc::Receiver<VmCommand>) -> Result<()> {
    debug!("Waiting for command...");
    let cmd = cmd_rx.recv()?;
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

    loop {
        if let Err(e) = handle_cmd(&mut vm, cmd_rx) {
            error!("Failed to handle command: {}", e);
            break;
        }
    }

    Ok(())
}

async fn create_vm(
    vm_config: MacosVmConfig,
) -> Result<(
    JoinHandle<Result<(), anyhow::Error>>,
    mpsc::Sender<VmCommand>,
)> {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<vm_rpc::VmCommand>();

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
                    info!("Removing existing aux socket path: {:?}", aux_sock_path);
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

    let vm_config_path = root_path.join("vm.json.base");
    let mut vm_config = load_vm_config(&vm_config_path)?;

    let shared_dir = root_path.join("shared");
    vm_config.shares = Some(vec![MacosVmSharedDirectory {
        path: shared_dir,
        automount: true,
        read_only: false,
    }]);

    let console_path = opts.console_sock;
    vm_config.serial = console_path.map(|path| MacosVmSerial { path });

    info!("Creating VM from config file: {:?}", vm_config_path);
    let (thread, cmd_tx) = create_vm(vm_config).await?;

    info!("Starting VM");
    cmd_tx.send(vm_rpc::VmCommand::Start)?;

    let aux_sock_path = format!("unix://{}", aux_sock_path.to_str().unwrap());
    let container_service = Box::new(ContainerService {
        state_map: Arc::new(RwLock::new(HashMap::new())),
        cmd_tx: cmd_tx.clone(),
    }) as Box<dyn ShimTask + Sync + Send>;
    let container_service = Arc::new(container_service);
    let container_task = create_task(container_service);

    loop {
        sleep(std::time::Duration::from_secs(30)).await;
        let agent_vsock_path = PathBuf::from(format!("/tmp/akari_vsock_{}", DEFAULT_AGENT_PORT));
        match agent_vsock_path.try_exists() {
            Ok(exist) => {
                if exist {
                    std::fs::remove_file(&agent_vsock_path).unwrap(); // TODO
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!(format!(
                    "Failed to check if the vsock path exists: {}",
                    e
                )));
            }
        }
        if cmd_tx
            .send(vm_rpc::VmCommand::Connect(
                DEFAULT_AGENT_PORT,
                agent_vsock_path.clone(),
            ))
            .is_ok()
        {
            info!("Connected to the agent");
            break;
        }
        sleep(std::time::Duration::from_secs(10)).await;
        info!("Retrying to connect to the agent");
    }

    info!("Listening on: {:?}", aux_sock_path);
    let mut server = Server::new()
        .bind(&aux_sock_path)
        .unwrap()
        .register_service(container_task);

    info!("Starting the server");
    server.start().await?;

    info!("Waiting for the VM thread to finish");
    thread.await??;

    Ok(())
}
