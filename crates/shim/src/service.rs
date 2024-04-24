// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::sync::Arc;

use async_trait::async_trait;
use containerd_shim::{
    publisher::RemotePublisher, spawn, Config, DeleteResponse, Error, ExitSignal, Flags, Shim,
    StartOpts,
};

use crate::task::Task;

pub struct Service {
    exit: Arc<ExitSignal>,
}

#[async_trait]
impl Shim for Service {
    type T = Task;

    async fn new(_runtime_id: &str, _args: &Flags, _config: &mut Config) -> Self {
        Service {
            exit: Arc::new(ExitSignal::default()),
        }
    }

    async fn start_shim(&mut self, opts: StartOpts) -> Result<String, Error> {
        let grouping = opts.id.clone();
        let address = spawn(opts, &grouping, Vec::new()).await?;
        // TODO: launch the Akari server
        Ok(address)
    }

    async fn delete_shim(&mut self) -> Result<DeleteResponse, Error> {
        // TODO: delete the Akari server
        Ok(DeleteResponse::default())
    }

    async fn wait(&mut self) {
        self.exit.wait().await;
    }

    async fn create_task_service(&self, _publisher: RemotePublisher) -> Task {
        // TODO: Invoke the Akari client
        Task::new()
    }
}
