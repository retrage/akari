// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{os::unix::net::UnixStream, path::PathBuf};

use anyhow::Result;
use liboci_cli::Delete;

use crate::{api, traits::WriteTo};

pub fn delete(args: Delete, _root_path: PathBuf, vmm_sock: &mut UnixStream) -> Result<()> {
    let request = api::Request {
        container_id: args.container_id.clone(),
        command: api::Command::Delete,
        vm_config: None,
        bundle: None,
    };

    request.send(vmm_sock)?;

    Ok(())
}
