// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Start;
use tarpc::context;

use crate::api::BackendApiClient;

pub async fn start(args: Start, _root_path: PathBuf, client: &BackendApiClient) -> Result<()> {
    client.start(context::current(), args.container_id).await?;

    Ok(())
}
