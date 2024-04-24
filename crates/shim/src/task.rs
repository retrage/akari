// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{
    collections::HashMap,
    io::Write,
    os::{
        fd::AsRawFd,
        unix::{net::UnixStream, thread::JoinHandleExt},
    },
    path::PathBuf,
    sync::{
        mpsc::{self, Sender},
        Arc, RwLock,
    },
    thread,
};

use anyhow::Result;
use async_trait::async_trait;
use containerd_shim::{
    api::{
        ConnectRequest, ConnectResponse, CreateTaskRequest, CreateTaskResponse, DeleteRequest,
        Empty, KillRequest, StartRequest, StartResponse, StateRequest, StateResponse,
    },
    protos::api::Status,
    DeleteResponse, Error as TaskError, Task as ShimTask, TtrpcContext, TtrpcResult,
};
use libakari::{
    api::{self, Command},
    path::root_path,
    vm_config::{MacosVmConfig, MacosVmSerial, MacosVmSharedDirectory},
};

type VmThread = thread::JoinHandle<Result<()>>;
type VmThreadTx = Sender<Command>;

#[derive(Debug)]
struct VmState {
    bundle: PathBuf,
    status: api::VmStatus,

    thread: Option<VmThread>,
    tx: Option<VmThreadTx>,
}

fn to_status(status: api::VmStatus) -> Status {
    match status {
        api::VmStatus::Created => Status::CREATED,
        api::VmStatus::Running => Status::RUNNING,
        api::VmStatus::Stopped => Status::STOPPED,
        _ => Status::UNKNOWN,
    }
}

type VmStateMap = HashMap<String, VmState>;

pub struct Task {
    state_map: Arc<RwLock<VmStateMap>>,
}

#[async_trait]
impl ShimTask for Task {
    async fn connect(
        &self,
        _ctx: &TtrpcContext,
        _req: ConnectRequest,
    ) -> TtrpcResult<ConnectResponse> {
        Ok(ConnectResponse::new())
    }

    async fn create(
        &self,
        _ctx: &TtrpcContext,
        req: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        let container_id = req.id;
        let bundle = PathBuf::from(req.bundle);
        let console_socket = if req.terminal {
            Some(PathBuf::from(req.stdout))
        } else {
            None
        };
        let vm_config = self
            .do_create(bundle.clone(), console_socket)
            .map_err(|e| TaskError::Other(e.to_string()))?;

        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| TaskError::Other("Lock poisoned".to_string()))?;

        if state_map.contains_key(&container_id) {
            return Err(containerd_shim::protos::ttrpc::Error::Others(
                "Container already exists".to_string(),
            ));
        }

        let state = VmState {
            bundle: bundle.clone(),
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
                let cmd = cmd_rx.recv()?;
                match cmd {
                    api::Command::Start => vm.start()?,
                    api::Command::Kill => vm.kill()?,
                    api::Command::Connect(port) => vm.connect(port, vsock_handler)?,
                    _ => {
                        break;
                    }
                }
            }
            Ok(())
        });

        let resp = CreateTaskResponse {
            pid: thread.as_pthread_t() as u32,
            ..Default::default()
        };

        if let Ok(status) = rx.recv() {
            let state = state_map
                .get_mut(&container_id.clone())
                .ok_or(TaskError::Other("Container not found".to_string()))?;
            state.status = status;
            state.thread = Some(thread);
            state.tx = Some(cmd_tx);
        }

        Ok(resp)
    }

    async fn delete(&self, _ctx: &TtrpcContext, req: DeleteRequest) -> TtrpcResult<DeleteResponse> {
        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| TaskError::Other("Lock poisoned".to_string()))?;
        let state = state_map
            .get_mut(req.id())
            .ok_or(TaskError::Other("Container not found".to_string()))?;

        match state.status {
            api::VmStatus::Created | api::VmStatus::Stopped => {
                let resp = DeleteResponse {
                    pid: state.thread.as_ref().unwrap().as_pthread_t() as u32,
                    ..Default::default()
                };
                state_map.remove(req.id());
                Ok(resp)
            }
            _ => Err(containerd_shim::protos::ttrpc::Error::Others(
                "Unexpected container status".to_string(),
            )),
        }
    }

    async fn kill(&self, _ctx: &TtrpcContext, req: KillRequest) -> TtrpcResult<Empty> {
        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| TaskError::Other("Lock poisoned".to_string()))?;
        let state = state_map
            .get_mut(req.id())
            .ok_or(TaskError::Other("Container not found".to_string()))?;

        match state.status {
            api::VmStatus::Running => {
                state
                    .tx
                    .as_ref()
                    .ok_or(TaskError::Other("Thread not found".to_string()))?
                    .send(api::Command::Kill)
                    .unwrap(); // TODO
                state.status = api::VmStatus::Stopped;
            }
            _ => {
                return Err(containerd_shim::protos::ttrpc::Error::Others(
                    "Unexpected container status".to_string(),
                ));
            }
        }

        Ok(Empty::new())
    }

    async fn start(&self, _ctx: &TtrpcContext, req: StartRequest) -> TtrpcResult<StartResponse> {
        let mut state_map = self
            .state_map
            .write()
            .map_err(|_| TaskError::Other("Lock poisoned".to_string()))?;
        let state = state_map
            .get_mut(req.id())
            .ok_or(TaskError::Other("Container not found".to_string()))?;

        match state.status {
            api::VmStatus::Created => {
                state
                    .tx
                    .as_ref()
                    .ok_or(TaskError::Other("Thread not found".to_string()))?
                    .send(api::Command::Start)
                    .unwrap(); // TODO
                state.status = api::VmStatus::Running;
            }
            _ => {
                return Err(containerd_shim::protos::ttrpc::Error::Others(
                    "Unexpected container status".to_string(),
                ));
            }
        }

        let resp = StartResponse {
            pid: state.thread.as_ref().unwrap().as_pthread_t() as u32,
            ..Default::default()
        };

        Ok(resp)
    }

    async fn state(&self, _ctx: &TtrpcContext, req: StateRequest) -> TtrpcResult<StateResponse> {
        let state_map = self
            .state_map
            .read()
            .map_err(|_| TaskError::Other("Lock poisoned".to_string()))?;
        let state = state_map
            .get(&req.id)
            .ok_or(TaskError::Other("Container not found".to_string()))?;

        let resp = StateResponse {
            id: req.id.clone(),
            bundle: state.bundle.clone().to_string_lossy().to_string(),
            pid: state.thread.as_ref().unwrap().as_pthread_t() as u32,
            status: to_status(state.status.clone()).into(),
            exec_id: req.exec_id.clone(),
            ..Default::default()
        };

        Ok(resp)
    }
}

impl Task {
    pub fn new() -> Self {
        Task {
            state_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn do_create(&self, bundle: PathBuf, console_socket: Option<PathBuf>) -> Result<MacosVmConfig> {
        let root_path = root_path(None)?;

        let base_vm_config_path = root_path.join("vm.json.base");
        let mut vm_config = libakari::vm_config::load_vm_config(&base_vm_config_path)?;

        assert!(vm_config.shares.is_none());

        let spec_path = bundle.join("config.json");
        assert!(spec_path.exists());
        let spec: oci_spec::runtime::Spec =
            serde_json::from_str(&std::fs::read_to_string(spec_path)?)?;

        let (root_path, read_only) = if let Some(root) = spec.root() {
            let root_path = if root.path().is_relative() {
                bundle.join(root.path()).canonicalize()?
            } else {
                root.path().canonicalize()?
            };
            let read_only = root.readonly().unwrap_or(false);
            (root_path, read_only)
        } else {
            return Err(anyhow::anyhow!("Root path is not specified"));
        };

        let rootfs = MacosVmSharedDirectory {
            path: root_path.clone(),
            automount: true,
            read_only,
        };
        vm_config.shares = Some(vec![rootfs]);

        if let Some(console_socket) = console_socket {
            let serial = MacosVmSerial {
                path: console_socket,
            };
            vm_config.serial = Some(serial);
        }

        Ok(vm_config)
    }
}
