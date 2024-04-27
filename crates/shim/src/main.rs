// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

mod service;
mod task;

use containerd_shim::asynchronous::run;

use service::Service;

#[tokio::main]
async fn main() {
    // simplelog::WriteLogger::init(
    //     simplelog::LevelFilter::Info,
    //     simplelog::Config::default(),
    //     std::fs::File::create("/tmp/shim.log").unwrap(),
    // ).unwrap();

    run::<Service>("io.containerd.akari.v1", None).await;
}
