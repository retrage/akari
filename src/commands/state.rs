// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use liboci_cli::State;
use serde::{Deserialize, Serialize};

use crate::vmm;

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
    pid: Option<u32>,
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

pub fn state(args: State, root_path: PathBuf) -> Result<()> {
    let config_path = root_path.join(format!("{}.json", args.container_id));
    let pid_path = root_path.join(format!("{}.pid", args.container_id));

    let mut pid: Option<u32> = None;
    let status = if config_path.exists() {
        if pid_path.exists() {
            pid = std::fs::read_to_string(&pid_path)
                .ok()
                .and_then(|s| s.trim().parse().ok());
            ContainerStatus::Running
        } else {
            ContainerStatus::Created
        }
    } else {
        ContainerStatus::Stopped
    };

    let vm_config = vmm::config::load_vm_config(&config_path)?;
    let share = vm_config
        .shares
        .first()
        .ok_or(anyhow::anyhow!("Bundle path not found"))?;
    let bundle = share.path.clone();

    let mut state = ContainerState::new(args.container_id, status, bundle);
    state.pid = pid;

    println!("{}", serde_json::to_string_pretty(&state)?);
    std::process::exit(0);
}
