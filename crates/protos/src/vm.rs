// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo
// Copyright The containerd Authors.

pub mod vm {
    include!(concat!(env!("OUT_DIR"), "/vm/vm.rs"));
}

pub mod vm_ttrpc {
    include!(concat!(env!("OUT_DIR"), "/vm/vm_ttrpc.rs"));
}

#[cfg(feature = "async")]
pub mod vm_ttrpc_async {
    include!(concat!(env!("OUT_DIR"), "/vm_async/vm_ttrpc.rs"));
}

pub(crate) mod empty {
    pub use crate::types::empty::*;
}
