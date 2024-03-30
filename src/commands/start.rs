// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{os::unix::net::UnixStream, path::PathBuf};

use anyhow::Result;
use liboci_cli::Start;

use crate::vmm::{config, start};

pub fn start(args: Start, root_path: PathBuf) -> Result<()> {
    // TODO: Create a VM on create
    let vm_config_path = root_path.join(format!("{}.json", args.container_id));
    if !vm_config_path.exists() {
        return Err(anyhow::anyhow!("VM configuration does not exist"));
    }
    let vm_config = config::load_vm_config(&vm_config_path)?;

    let serial_sock = match &vm_config.serial {
        Some(serial) => Some(UnixStream::connect(&serial.path)?),
        None => None,
    };

    let config = start::create_vm(vm_config, &serial_sock)?;

    unsafe {
        start::start_vm(config);
    }

    Ok(())
}
