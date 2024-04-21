// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

mod service;
mod task;

use containerd_shim::asynchronous::run;

use service::Service;

#[tokio::main]
async fn main() {
    run::<Service>("io.containerd.akari.v2", None).await;
}
