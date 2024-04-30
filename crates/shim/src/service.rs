// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::sync::Arc;

use async_trait::async_trait;
use containerd_shim::{
    protos::shim_async::{Client, TaskClient},
    publisher::RemotePublisher,
    spawn, Config, DeleteResponse, Error, ExitSignal, Flags, Shim, StartOpts,
};
use libakari::path::{aux_sock_path, root_path};

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
        // TODO: Check if the VM server is running
        // TODO: Connect to the VM server and request a connection to the VM agent
        let grouping = opts.id.clone();
        let address = spawn(opts, &grouping, Vec::new()).await?;
        Ok(address)
    }

    async fn delete_shim(&mut self) -> Result<DeleteResponse, Error> {
        Ok(DeleteResponse::default())
    }

    async fn wait(&mut self) {
        self.exit.wait().await;
    }

    async fn create_task_service(&self, _publisher: RemotePublisher) -> Task {
        // TODO: Get the root path and the auxiliary socket path
        let root_path = root_path(None).unwrap();
        let aux_sock_path = aux_sock_path(&root_path, None);

        let client = TaskClient::new(Client::connect(aux_sock_path.to_str().unwrap()).unwrap());

        Task { client }
    }
}
