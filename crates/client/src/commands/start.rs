// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use libakari::vm_rpc::VmRpcClient;
use liboci_cli::Start;
use tarpc::context;

use super::error::Error;

pub async fn start(args: Start, _root_path: PathBuf, client: &VmRpcClient) -> Result<(), Error> {
    client
        .start(context::current(), args.container_id)
        .await
        .map_err(Error::RpcClient)?
        .map_err(Error::Api)?;
    Ok(())
}
