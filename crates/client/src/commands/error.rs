// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Container configuration does not exist")]
    ContainerConfigDoesNotExist,
    #[error("Root path is not specified")]
    RootfsPathIsNotSpecified,
    #[error(transparent)]
    VmConfig(#[from] libakari::vm_config::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Api(#[from] libakari::vm_rpc::Error),
    #[error(transparent)]
    Deserialize(#[from] serde_json::Error),
    #[error(transparent)]
    RpcClient(#[from] tarpc::client::RpcError),
}
