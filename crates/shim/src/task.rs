// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use async_trait::async_trait;
use containerd_shim::{
    api::{
        ConnectRequest, ConnectResponse, CreateTaskRequest, CreateTaskResponse, DeleteRequest,
        Empty, KillRequest, StartRequest, StartResponse, StateRequest, StateResponse,
    },
    protos::api::Status,
    DeleteResponse, Task as ShimTask, TtrpcContext, TtrpcResult,
};
use libakari::{
    api::{self, CreateRequest},
    path::{root_path, vmm_sock_path},
};
use log::info;
use tarpc::{context, serde_transport, tokio_serde::formats::Json};

fn to_status(status: api::VmStatus) -> Status {
    match status {
        api::VmStatus::Created => Status::CREATED,
        api::VmStatus::Running => Status::RUNNING,
        api::VmStatus::Stopped => Status::STOPPED,
        _ => Status::UNKNOWN,
    }
}

pub struct Task {
    client: api::ApiClient,
}

#[async_trait]
impl ShimTask for Task {
    async fn connect(
        &self,
        _ctx: &TtrpcContext,
        req: ConnectRequest,
    ) -> TtrpcResult<ConnectResponse> {
        info!("connect: {:?}", req);

        self.client
            .connect(context::current(), req.id, 1234 /* TODO */)
            .await
            .unwrap()
            .unwrap();

        Ok(ConnectResponse::new())
    }

    async fn create(
        &self,
        _ctx: &TtrpcContext,
        req: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        info!("create: {:?}", req);
        let container_id = req.id;
        let rootfs = req.rootfs[0].source.clone();

        let req = CreateRequest {
            bundle: PathBuf::from(req.bundle),
            rootfs: PathBuf::from(rootfs),
            stdin: Some(PathBuf::from(req.stdin)),
            stdout: Some(PathBuf::from(req.stdout)),
            stderr: Some(PathBuf::from(req.stderr)),
        };

        self.client
            .create(context::current(), container_id, req)
            .await
            .unwrap()
            .unwrap();

        let resp = CreateTaskResponse {
            pid: 1234, // TODO
            ..Default::default()
        };

        Ok(resp)
    }

    async fn delete(&self, _ctx: &TtrpcContext, req: DeleteRequest) -> TtrpcResult<DeleteResponse> {
        info!("delete: {:?}", req);

        self.client
            .delete(context::current(), req.id)
            .await
            .unwrap()
            .unwrap();

        let resp = DeleteResponse {
            pid: 1234,      // TODO
            exit_status: 0, // TODO
            ..Default::default()
        };

        Ok(resp)
    }

    async fn kill(&self, _ctx: &TtrpcContext, req: KillRequest) -> TtrpcResult<Empty> {
        info!("kill: {:?}", req);

        self.client
            .kill(context::current(), req.id)
            .await
            .unwrap()
            .unwrap();

        Ok(Empty::new())
    }

    async fn start(&self, _ctx: &TtrpcContext, req: StartRequest) -> TtrpcResult<StartResponse> {
        info!("start: {:?}", req);

        self.client
            .start(context::current(), req.id)
            .await
            .unwrap()
            .unwrap();

        let resp = StartResponse {
            pid: 1234, // TODO
            ..Default::default()
        };

        Ok(resp)
    }

    async fn state(&self, _ctx: &TtrpcContext, req: StateRequest) -> TtrpcResult<StateResponse> {
        info!("state: {:?}", req);

        let r = self
            .client
            .state(context::current(), req.id.clone())
            .await
            .unwrap()
            .unwrap();

        let resp = StateResponse {
            id: req.id.clone(),
            exec_id: req.exec_id.clone(),
            bundle: r.bundle.to_str().unwrap().to_string(),
            pid: r.pid.unwrap_or(1234) as u32,
            status: to_status(r.status).into(),
            ..Default::default()
        };

        Ok(resp)
    }
}

impl Task {
    pub async fn new() -> Self {
        let root_path = root_path(None).unwrap();
        let vmm_sock_path = vmm_sock_path(&root_path, None);

        let transport = serde_transport::unix::connect(vmm_sock_path, Json::default);
        let client =
            api::ApiClient::new(tarpc::client::Config::default(), transport.await.unwrap()).spawn();

        Task { client }
    }
}
