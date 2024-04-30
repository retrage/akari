// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use anyhow::Result;
use containerd_shim::{api::KillRequest, protos::shim_async::TaskClient, Context};
use liboci_cli::Kill;

use super::error::Error;

pub async fn kill(args: Kill, client: &TaskClient) -> Result<(), Error> {
    let ctx = Context::default();
    let req = KillRequest {
        id: args.container_id,
        ..Default::default()
    };
    let _ = client.kill(ctx, &req).await.map_err(Error::RpcClient)?;
    Ok(())
}
