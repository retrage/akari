// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use anyhow::Result;
use clap::Parser;
use containerd_shim::{
    protos::shim::{shim::ConnectRequest, shim_ttrpc_async::TaskClient},
    Context,
};

use super::error::Error;

/// Connect to a running container
#[derive(Parser, Debug)]
pub struct Connect {
    container_id: String,
    port: u32,
}

pub async fn connect(args: Connect, client: &TaskClient) -> Result<(), Error> {
    let ctx = Context::default();
    let req = ConnectRequest {
        id: args.container_id,
        ..Default::default()
    };
    let _ = client.connect(ctx, &req).await.map_err(Error::RpcClient)?;
    Ok(())
}
