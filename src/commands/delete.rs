// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Delete;
use tarpc::context;

use crate::api::ApiClient;

pub async fn delete(args: Delete, _root_path: PathBuf, client: &ApiClient) -> Result<()> {
    client.delete(context::current(), args.container_id).await?;

    Ok(())
}
