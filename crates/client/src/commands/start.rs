// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use anyhow::Result;
use containerd_shim::{api::StartRequest, protos::shim_async::TaskClient, Context};
use liboci_cli::Start;

use super::error::Error;

pub async fn start(args: Start, client: &TaskClient) -> Result<(), Error> {
    let ctx = Context::default();
    let req = StartRequest {
        id: args.container_id,
        ..Default::default()
    };
    let _ = client.start(ctx, &req).await.map_err(Error::RpcClient)?;
    Ok(())
}
