// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo
// Copyright The containerd Authors.

#![allow(warnings)]

pub use protobuf;
pub use ttrpc;

pub mod types;
pub mod vm;

pub mod vm_sync {
    pub use ttrpc::Client;

    pub use crate::vm::vm_ttrpc::{create_vm_service, VmServiceClient};
}

#[cfg(feature = "async")]
pub mod vm_async {
    pub use ttrpc::asynchronous::Client;

    pub use crate::vm::vm_ttrpc_async::{create_vm_service, VmServiceClient};
}
