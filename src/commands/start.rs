// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Start;

use crate::vmm::start;

pub fn start(args: Start, root_path: PathBuf) -> Result<()> {
    // TODO: Create a VM on create
    let config = start::create_vm(&root_path, &args.container_id)?;

    unsafe {
        start::start_vm(config);
    }

    Ok(())
}
