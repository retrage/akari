// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Delete;

pub fn delete(args: Delete, root_path: PathBuf) -> Result<()> {
    let vm_config_path = root_path.join(format!("{}.json", args.container_id));
    if !vm_config_path.exists() {
        return Err(anyhow::anyhow!("VM configuration does not exist"));
    }

    // TODO: Check if the VM is running

    std::fs::remove_file(vm_config_path)?;

    Ok(())
}
