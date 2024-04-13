// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use liboci_cli::State;
use serde::{Deserialize, Serialize};
use tarpc::context;

use crate::api::{self, BackendApiClient};

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

impl From<api::VmStatus> for ContainerStatus {
    fn from(status: api::VmStatus) -> Self {
        match status {
            api::VmStatus::Creating => ContainerStatus::Creating,
            api::VmStatus::Created => ContainerStatus::Created,
            api::VmStatus::Running => ContainerStatus::Running,
            api::VmStatus::Stopped => ContainerStatus::Stopped,
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
    bundle: PathBuf,
    // annotations associated with the container
    #[serde(skip_serializing_if = "Option::is_none")]
    annotations: Option<HashMap<String, String>>,
}

impl ContainerState {
    pub fn new(id: String, status: ContainerStatus, bundle: PathBuf) -> Self {
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

pub async fn state(args: State, _root_path: PathBuf, client: &BackendApiClient) -> Result<()> {
    let response = client.state(context::current(), args.container_id).await?;

    let status = ContainerStatus::from(response.status);
    let bundle = response.bundle;

    let mut state = ContainerState::new(response.container_id, status, bundle);
    state.pid = response.pid;

    println!("{}", serde_json::to_string_pretty(&state)?);
    std::process::exit(0);
}
