// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Kill;
use tarpc::context;

use crate::api::ApiClient;

use super::error::Error;

pub async fn kill(args: Kill, _root_path: PathBuf, client: &ApiClient) -> Result<(), Error> {
    client
        .kill(context::current(), args.container_id)
        .await
        .map_err(Error::RpcClient)?
        .map_err(Error::Api)?;

    Ok(())
}
