// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Create;
use tarpc::context;

use libakari::api::{ApiClient, CreateRequest};

use super::error::Error;

pub async fn create(args: Create, _root_path: PathBuf, client: &ApiClient) -> Result<(), Error> {
    let spec_path = args.bundle.join("config.json");
    if !spec_path.exists() {
        return Err(Error::ContainerConfigDoesNotExist);
    }
    let spec: oci_spec::runtime::Spec = serde_json::from_str(&std::fs::read_to_string(spec_path)?)?;

    let rootfs_path = if let Some(root) = spec.root() {
        if root.path().is_relative() {
            args.bundle.join(root.path()).canonicalize()?
        } else {
            root.path().canonicalize()?
        }
    } else {
        return Err(Error::RootfsPathIsNotSpecified);
    };

    let req = CreateRequest {
        bundle: args.bundle.clone(),
        rootfs: rootfs_path,
        stdin: args.console_socket.clone(),
        stdout: args.console_socket.clone(),
        stderr: None,
    };

    client
        .create(context::current(), args.container_id, req)
        .await
        .map_err(Error::RpcClient)?
        .map_err(Error::Api)?;

    Ok(())
}
