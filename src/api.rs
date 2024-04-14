// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::vmm::api::MacosVmConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Create,
    Delete,
    Kill,
    Start,
    State,
    Connect(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VmStatus {
    Creating,
    Created,
    Running,
    Stopped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub container_id: String,
    pub command: Command,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_config: Option<MacosVmConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub container_id: String,
    pub status: VmStatus,
    pub pid: Option<i32>,
    pub config: MacosVmConfig,
    pub bundle: PathBuf,
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum Error {
    #[error("Container already exists")]
    ContainerAlreadyExists,
    #[error("Container not found")]
    ContainerNotFound,
    #[error("Unpextected container status: {0:?}")]
    UnpextectedContainerStatus(VmStatus),
    #[error("Lock poisoned")]
    LockPoisoned,
    #[error("Thread not found")]
    ThreadNotFound,
    #[error("Failed to send command")]
    VmCommandFailed,
}

#[tarpc::service]
pub trait Api {
    async fn create(
        container_id: String,
        vm_config: MacosVmConfig,
        bundle: PathBuf,
    ) -> Result<(), Error>;
    async fn delete(container_id: String) -> Result<(), Error>;
    async fn kill(container_id: String) -> Result<(), Error>;
    async fn start(container_id: String) -> Result<(), Error>;
    async fn state(container_id: String) -> Result<Response, Error>;
    async fn connect(container_id: String, port: u32) -> Result<(), Error>;
}
