// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Create;

use crate::vmm;

pub fn create(args: Create, root_path: PathBuf) -> Result<()> {
    println!("create: {:?}", args.bundle);

    // Open base vm config in root_path
    let base_config_path = root_path.join("vm.json");
    let mut config = vmm::config::load_vm_config(&base_config_path)?;

    assert!(config.shares.is_empty());

    let share = vmm::config::MacosVmSharedDirectory {
        path: args.bundle,
        automount: true,
        read_only: false,
    };
    config.shares.push(share);

    // TODO: Support pid_file
    // TODO: Support console_socket

    // Save the new VM configuration
    let config_path = root_path.join(format!("{}.json", args.container_id));
    let config_json = serde_json::to_string(&config)?;
    std::fs::write(config_path, config_json)?;

    // TODO: Ask the VMM to create the VM

    Ok(())
}
