// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Guest Agent
//!
//! The Akari Guest Agent is a daemon that runs inside the guest VM. It listens for requests from the host and performs operations on the guest VM.
//! 1. Listen on a vsock port for commands from the host.
//! 2. Create a listener thread for each incoming connection. The listener thread corresponds to a container.
//!   - The listener is exposed to the host as a Unix domain socket.
//!   - The socket accepts TTRPC requests from the host.

use std::{
    collections::HashMap,
    os::fd::{AsFd, AsRawFd},
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use containerd_shim::{
    api::{
        ConnectRequest, ConnectResponse, CreateTaskRequest, CreateTaskResponse, DeleteRequest,
        Empty, KillRequest, StartRequest, StartResponse, StateRequest, StateResponse,
    },
    DeleteResponse, Task as ShimTask, TtrpcContext, TtrpcResult,
};
use containerd_shim_protos::shim_async::create_task;
use libakari::agent::DEFAULT_AGENT_PORT;
use log::debug;
use tokio::{
    signal::unix::{signal, SignalKind},
    sync::RwLock,
};
use tokio_vsock::{VsockAddr, VsockListener, VMADDR_CID_ANY};
use ttrpc::asynchronous::Server;

struct ContainerState {}

type ContainerStateMap = HashMap<String, ContainerState>;

#[derive(Clone)]
struct AgentService {
    state_map: Arc<RwLock<ContainerStateMap>>,
}

#[async_trait]
impl ShimTask for AgentService {
    async fn connect(
        &self,
        _ctx: &TtrpcContext,
        _req: ConnectRequest,
    ) -> TtrpcResult<ConnectResponse> {
        todo!()
    }

    async fn create(
        &self,
        _ctx: &TtrpcContext,
        req: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        debug!("CreateTaskRequest: {:?}", req);
        let options = req.options().to_string();
        debug!("Options: {:?}", options);
        if req.options().type_url == "akari.io/vsock_port" {
            let vsock_port = req.options().value.clone();
            let vsock_port = u32::from_le_bytes(vsock_port.try_into().unwrap());
            debug!("vsock_port: {:?}", vsock_port);
        }
        todo!()
    }

    async fn delete(
        &self,
        _ctx: &TtrpcContext,
        _req: DeleteRequest,
    ) -> TtrpcResult<DeleteResponse> {
        // let mut state_map = self.state_map.write().await;
        // state_map.remove(req.id());
        // let res = DeleteResponse::default();
        // TODO: Fill in the response
        // Ok(res)
        todo!()
    }

    async fn kill(&self, _ctx: &TtrpcContext, _req: KillRequest) -> TtrpcResult<Empty> {
        // let mut state_map = self.state_map.write().await;
        todo!()
    }

    async fn start(&self, _ctx: &TtrpcContext, _req: StartRequest) -> TtrpcResult<StartResponse> {
        // let mut state_map = self.state_map.write().await;
        todo!()
    }

    async fn state(&self, _ctx: &TtrpcContext, _req: StateRequest) -> TtrpcResult<StateResponse> {
        // let mut state_map = self.state_map.write().await;
        todo!()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::init();

    let agent_service = Box::new(AgentService {
        state_map: Arc::new(RwLock::new(HashMap::new())),
    }) as Box<dyn ShimTask + Sync + Send>;
    let agent_service = Arc::new(agent_service);
    let agent_task = create_task(agent_service);

    let addr = VsockAddr::new(VMADDR_CID_ANY, DEFAULT_AGENT_PORT);
    let listener = VsockListener::bind(addr)?;
    let mut server = Server::new()
        .set_domain_vsock()
        .add_listener(listener.as_fd().as_raw_fd())
        .unwrap()
        .register_service(agent_task);

    let mut hangup = signal(SignalKind::hangup()).unwrap();
    let mut interrupt = signal(SignalKind::interrupt()).unwrap();

    server.start().await?;

    tokio::select! {
        _ = hangup.recv() => {
            log::info!("Received hangup signal");
            server.stop_listen().await;
            server.start().await?;
        }
        _ = interrupt.recv() => {
            log::info!("Received interrupt signal");
            server.shutdown().await?;
        }
    }

    Ok(())
}
