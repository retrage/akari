// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # containerd-shim-akari-v2
//! This is a containerd shim v2 implementation for Akari.
//! It is just a simple shim that forwards the requests to the Unix domain socket.

mod service;
mod task;

use containerd_shim::asynchronous::run;

use service::Service;

#[tokio::main]
async fn main() {
    run::<Service>("io.containerd.akari.v2", None).await;
}
