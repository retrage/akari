// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use async_trait::async_trait;
use containerd_shim::{
    api::{
        ConnectRequest, ConnectResponse, CreateTaskRequest, CreateTaskResponse, DeleteRequest,
        Empty, KillRequest, StartRequest, StartResponse, StateRequest, StateResponse,
    },
    DeleteResponse, Task as ShimTask, TtrpcContext, TtrpcResult,
};

pub struct Task;

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
        _: CreateTaskRequest,
    ) -> TtrpcResult<CreateTaskResponse> {
        Ok(CreateTaskResponse::default())
    }

    async fn delete(&self, _ctx: &TtrpcContext, _: DeleteRequest) -> TtrpcResult<DeleteResponse> {
        Ok(DeleteResponse::default())
    }

    async fn kill(&self, _ctx: &TtrpcContext, _: KillRequest) -> TtrpcResult<Empty> {
        Ok(Empty::new())
    }

    async fn start(&self, _ctx: &TtrpcContext, _: StartRequest) -> TtrpcResult<StartResponse> {
        Ok(StartResponse::default())
    }

    async fn state(&self, _ctx: &TtrpcContext, _req: StateRequest) -> TtrpcResult<StateResponse> {
        Ok(StateResponse::default())
    }
}
