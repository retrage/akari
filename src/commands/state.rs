// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{collections::HashMap, os::unix::net::UnixStream, path::PathBuf};

use anyhow::Result;
use liboci_cli::State;
use serde::{Deserialize, Serialize};

use crate::{
    api,
    traits::{ReadFrom, WriteTo},
};

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

pub fn state(args: State, _root_path: PathBuf, vmm_sock: &mut UnixStream) -> Result<()> {
    let request = api::Request {
        container_id: args.container_id.clone(),
        command: api::Command::State,
        vm_config: None,
    };
    request.send(vmm_sock)?;

    let response = api::Response::recv(vmm_sock)?;

    let status = match response.status {
        api::VmStatus::Created => ContainerStatus::Created,
        api::VmStatus::Running => ContainerStatus::Running,
        api::VmStatus::Stopped => ContainerStatus::Stopped,
        _ => ContainerStatus::Creating,
    };

    // TODO
    let bundle = response
        .config
        .shares
        .unwrap()
        .first()
        .unwrap()
        .path
        .clone();

    let mut state = ContainerState::new(args.container_id, status, bundle);
    state.pid = response.pid;

    println!("{}", serde_json::to_string_pretty(&state)?);
    std::process::exit(0);
}
