// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Start;
use tarpc::context;

use crate::api::ApiClient;

use super::error::Error;

pub async fn start(args: Start, _root_path: PathBuf, client: &ApiClient) -> Result<(), Error> {
    client
        .start(context::current(), args.container_id)
        .await
        .map_err(Error::RpcClientError)?
        .map_err(Error::Api)?;
    Ok(())
}
