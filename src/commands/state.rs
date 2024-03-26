// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use liboci_cli::State;

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

pub fn state(args: State, _root_path: PathBuf) -> std::io::Result<()> {
    // TODO: Find the container with args.id and print its state
    println!("state: {}", args.container_id);
    Ok(())
}
