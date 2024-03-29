// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Kill;

pub fn kill(args: Kill, _root_path: PathBuf) -> Result<()> {
    println!("Kill: {}", args.container_id);
    Ok(())
}
