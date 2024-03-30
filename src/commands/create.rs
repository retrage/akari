// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Create;

use crate::vmm;

pub fn create(args: Create, root_path: PathBuf) -> Result<()> {
    let vm_config_path = root_path.join(format!("{}.json", args.container_id));
    if vm_config_path.exists() {
        return Err(anyhow::anyhow!("VM configuration already exists"));
    }

    // Open base vm config in root_path
    let base_vm_config_path = root_path.join("vm.json.base");
    let mut vm_config = vmm::config::load_vm_config(&base_vm_config_path)?;

    assert!(vm_config.shares.is_none());

    let spec_path = args.bundle.join("config.json");
    if !spec_path.exists() {
        return Err(anyhow::anyhow!("Container configuration does not exist"));
    }
    let spec: oci_spec::runtime::Spec = serde_json::from_str(&std::fs::read_to_string(spec_path)?)?;

    if let Some(root) = spec.root() {
        let root_path = if root.path().is_relative() {
            args.bundle.join(root.path()).canonicalize()?
        } else {
            root.path().canonicalize()?
        };
        let rootfs = vmm::config::MacosVmSharedDirectory {
            path: root_path,
            automount: true,
            read_only: root.readonly().unwrap_or(false),
        };
        vm_config.shares = Some(vec![rootfs]);
    } else {
        return Err(anyhow::anyhow!("Root path is not specified"));
    }

    // TODO: spec.process
    // TODO: spec.hostname
    // TODO: spec.mounts

    // TODO: Support pid_file
    // TODO: Support console_socket

    let config_json = serde_json::to_string_pretty(&vm_config)?;
    std::fs::write(vm_config_path, config_json)?; // TODO: Potential TOCTOU bug

    // TODO: Ask the VMM to create the VM

    Ok(())
}
