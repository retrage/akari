// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use tarpc::context;

use crate::api::ApiClient;

use super::error::Error;

/// Connect to a running container
#[derive(Parser, Debug)]
pub struct Connect {
    container_id: String,
    port: u32,
}

pub async fn connect(args: Connect, _root_path: PathBuf, client: &ApiClient) -> Result<(), Error> {
    client
        .connect(context::current(), args.container_id, args.port)
        .await
        .map_err(Error::RpcClientError)?
        .map_err(Error::Api)?;

    Ok(())
}
