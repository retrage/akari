// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use liboci_cli::Delete;

pub fn delete(args: Delete, _root_path: PathBuf) -> std::io::Result<()> {
    println!("Delete: {}", args.container_id);
    Ok(())
}
