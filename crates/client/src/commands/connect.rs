// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use libakari::vm_rpc::VmRpcClient;
use tarpc::context;

use super::error::Error;

/// Connect to a running container
#[derive(Parser, Debug)]
pub struct Connect {
    container_id: String,
    port: u32,
}

pub async fn connect(
    args: Connect,
    _root_path: PathBuf,
    client: &VmRpcClient,
) -> Result<(), Error> {
    client
        .connect(context::current(), args.container_id, args.port)
        .await
        .map_err(Error::RpcClient)?
        .map_err(Error::Api)?;

    Ok(())
}
