// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use anyhow::Result;
use containerd_shim::{
    protos::shim::{shim::DeleteRequest, shim_ttrpc_async::TaskClient},
    Context,
};
use liboci_cli::Delete;

use super::error::Error;

pub async fn delete(args: Delete, client: &TaskClient) -> Result<(), Error> {
    let ctx = Context::default();
    let req = DeleteRequest {
        id: args.container_id,
        ..Default::default()
    };
    let _ = client.delete(ctx, &req).await.map_err(Error::RpcClient)?;
    Ok(())
}
