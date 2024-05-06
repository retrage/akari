// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// Command to control the VM.
pub enum VmCommand {
    Start,
    Stop,
    Pause,
    Resume,
    Connect(u32, PathBuf),
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
