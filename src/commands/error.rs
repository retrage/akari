// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use crate::{api, vmm};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("VM configuration already exists")]
    VmConfigAlreadyExists,
    #[error("Container configuration does not exist")]
    ContainerConfigDoesNotExist,
    #[error("Root path is not specified")]
    RootPathIsNotSpecified,
    #[error(transparent)]
    VmmApiError(#[from] vmm::api::Error),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    Api(#[from] api::Error),
    #[error(transparent)]
    DeserializeError(#[from] serde_json::Error),
    #[error(transparent)]
    RpcClientError(#[from] tarpc::client::RpcError),
}
