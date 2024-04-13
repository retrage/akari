// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use liboci_cli::Create;
use tarpc::context;

use crate::{api::ApiClient, vmm};

use super::error::Error;

pub async fn create(args: Create, root_path: PathBuf, client: &ApiClient) -> Result<(), Error> {
    let vm_config_path = root_path.join(format!("{}.json", args.container_id));
    if vm_config_path.exists() {
        return Err(Error::VmConfigAlreadyExists);
    }

    // Open base vm config in root_path
    let base_vm_config_path = root_path.join("vm.json.base");
    let mut vm_config = vmm::api::load_vm_config(&base_vm_config_path)?;

    assert!(vm_config.shares.is_none());

    let spec_path = args.bundle.join("config.json");
    if !spec_path.exists() {
        return Err(Error::ContainerConfigDoesNotExist);
    }
    let spec: oci_spec::runtime::Spec = serde_json::from_str(&std::fs::read_to_string(spec_path)?)?;

    let (root_path, read_only) = if let Some(root) = spec.root() {
        let root_path = if root.path().is_relative() {
            args.bundle.join(root.path()).canonicalize()?
        } else {
            root.path().canonicalize()?
        };
        let read_only = root.readonly().unwrap_or(false);
        (root_path, read_only)
    } else {
        return Err(Error::RootPathIsNotSpecified);
    };

    let rootfs = vmm::api::MacosVmSharedDirectory {
        path: root_path.clone(),
        automount: true,
        read_only,
    };
    vm_config.shares = Some(vec![rootfs]);

    // Handle console_socket
    if let Some(console_socket) = args.console_socket {
        let serial = vmm::api::MacosVmSerial {
            path: console_socket,
        };
        vm_config.serial = Some(serial);
    }

    // TODO: spec.process
    // TODO: spec.hostname
    // TODO: spec.mounts

    // TODO: Support pid_file

    client
        .create(context::current(), args.container_id, vm_config, root_path)
        .await
        .map_err(Error::RpcClientError)?
        .map_err(Error::Api)?;

    Ok(())
}
