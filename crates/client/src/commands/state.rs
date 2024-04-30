// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::collections::HashMap;

use anyhow::Result;
use containerd_shim::{api::StateRequest, protos::shim_async::TaskClient, Context};
use liboci_cli::State;
use serde::{Deserialize, Serialize};

use super::error::Error;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ContainerStatus {
    // the container is being created
    Creating,
    // the runtime has finished the create operation
    Created,
    // the container is running
    Running,
    // the container has exited
    Stopped,
}

impl From<containerd_shim::api::Status> for ContainerStatus {
    fn from(val: containerd_shim::api::Status) -> Self {
        match val {
            containerd_shim::api::Status::CREATED => ContainerStatus::Created,
            containerd_shim::api::Status::RUNNING => ContainerStatus::Running,
            containerd_shim::api::Status::STOPPED => ContainerStatus::Stopped,
            _ => panic!("Invalid container status"),
        }
    }
}

/// OCI runtime state
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerState {
    // version of the Open Container Initiative specification
    oci_version: String,
    // container ID
    id: String,
    // runtime state of the container
    status: ContainerStatus,
    // ID of the container process
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<i32>,
    // absolute path to the container's bundle directory
    bundle: String,
    // annotations associated with the container
    #[serde(skip_serializing_if = "Option::is_none")]
    annotations: Option<HashMap<String, String>>,
}

impl ContainerState {
    pub fn new(id: String, status: ContainerStatus, bundle: String) -> Self {
        Self {
            oci_version: "v1.0.2".to_string(),
            id,
            status,
            pid: None,
            bundle,
            annotations: None,
        }
    }
}

pub async fn state(args: State, client: &TaskClient) -> Result<(), Error> {
    let ctx = Context::default();
    let req = StateRequest {
        id: args.container_id,
        ..Default::default()
    };
    let response = client.state(ctx, &req).await.map_err(Error::RpcClient)?;

    let status = response.status.unwrap().into();
    let bundle = response.bundle;

    let mut state = ContainerState::new(response.id, status, bundle);
    state.pid = match response.pid {
        0 => None,
        pid => Some(pid as i32),
    };

    println!("{}", serde_json::to_string_pretty(&state)?);
    std::process::exit(0);
}
