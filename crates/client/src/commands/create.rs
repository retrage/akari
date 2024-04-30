// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use anyhow::Result;
use containerd_shim::{
    protos::shim::{shim::CreateTaskRequest, shim_ttrpc_async::TaskClient},
    Context,
};
use liboci_cli::Create;

use super::error::Error;

pub async fn create(args: Create, client: &TaskClient) -> Result<(), Error> {
    let spec_path = args.bundle.join("config.json");
    if !spec_path.exists() {
        return Err(Error::ContainerConfigDoesNotExist);
    }
    let spec: oci_spec::runtime::Spec = serde_json::from_str(&std::fs::read_to_string(spec_path)?)?;

    // TODO: Needs to convert to the guest path
    let _rootfs_path = if let Some(root) = spec.root() {
        if root.path().is_relative() {
            args.bundle.join(root.path()).canonicalize()?
        } else {
            root.path().canonicalize()?
        }
    } else {
        return Err(Error::RootfsPathIsNotSpecified);
    };

    // TODO: Needs to convert to the guest path
    let bundle = args.bundle.to_str().unwrap();
    let (terminal, stdin, stdout) = match args.console_socket {
        Some(ref console_socket) => (
            true,
            console_socket.to_str().unwrap(),
            console_socket.to_str().unwrap(),
        ),
        None => (false, "", ""),
    };

    let ctx = Context::default();
    let req = CreateTaskRequest {
        id: args.container_id,
        bundle: bundle.to_string(),
        terminal,
        stdin: stdin.to_string(),
        stdout: stdout.to_string(),
        ..Default::default()
    };

    let _ = client.create(ctx, &req).await.map_err(Error::RpcClient)?;
    Ok(())
}
