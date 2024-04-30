// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use async_trait::async_trait;
use containerd_shim::{
    api::{
        ConnectRequest, ConnectResponse, CreateTaskRequest, CreateTaskResponse, DeleteRequest,
        Empty, KillRequest, StartRequest, StartResponse, StateRequest, StateResponse,
    },
    protos::shim_async::TaskClient,
    Context, DeleteResponse, Task as ShimTask, TtrpcContext, TtrpcResult,
};

pub struct Task {
    pub client: TaskClient,
}

#[async_trait]
impl ShimTask for Task {
    async fn connect(
        &self,
        _ctx: &TtrpcContext,
        req: ConnectRequest,
    ) -> TtrpcResult<ConnectResponse> {
        Ok(self.client.connect(Context::default(), &req).await?)
    }

    async fn create(
        &self,
        _ctx: &TtrpcContext,
        req: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        Ok(self.client.create(Context::default(), &req).await?)
    }

    async fn delete(&self, _ctx: &TtrpcContext, req: DeleteRequest) -> TtrpcResult<DeleteResponse> {
        Ok(self.client.delete(Context::default(), &req).await?)
    }

    async fn kill(&self, _ctx: &TtrpcContext, req: KillRequest) -> TtrpcResult<Empty> {
        Ok(self.client.kill(Context::default(), &req).await?)
    }

    async fn start(&self, _ctx: &TtrpcContext, req: StartRequest) -> TtrpcResult<StartResponse> {
        Ok(self.client.start(Context::default(), &req).await?)
    }

    async fn state(&self, _ctx: &TtrpcContext, req: StateRequest) -> TtrpcResult<StateResponse> {
        Ok(self.client.state(Context::default(), &req).await?)
    }
}
