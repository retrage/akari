// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub enum Command {
    Create,
    Delete,
    Kill,
    Start,
    State,
    Connect(u32),
    Disconnect(u32),
    VsockSend(u32, Vec<u8>),
    VsockRecv(u32),
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
pub struct Response {
    pub container_id: String,
    pub status: VmStatus,
    pub pid: Option<i32>,
    pub bundle: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    pub bundle: PathBuf,
    pub rootfs: PathBuf,
    pub stdin: Option<PathBuf>,
    pub stdout: Option<PathBuf>,
    pub stderr: Option<PathBuf>,
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
    async fn create(container_id: String, args: CreateRequest) -> Result<(), Error>;
    async fn delete(container_id: String) -> Result<(), Error>;
    async fn kill(container_id: String) -> Result<(), Error>;
    async fn start(container_id: String) -> Result<(), Error>;
    async fn state(container_id: String) -> Result<Response, Error>;
    async fn connect(container_id: String, port: u32) -> Result<(), Error>;
}
