// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Delete;

pub fn delete(args: Delete, _root_path: PathBuf) -> Result<()> {
    println!("Delete: {}", args.container_id);
    Ok(())
}
