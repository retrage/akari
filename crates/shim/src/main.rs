// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # containerd-shim-akari-v2
//! This is a containerd shim v2 implementation for Akari.
//! It is just a simple shim that forwards the requests to the Unix domain socket.

mod service;
mod task;

// use std::io::BufRead;

use anyhow::Result;
use containerd_shim::asynchronous::run;
// use containerd_shim_logging::{Config, Driver};

use service::Service;
// use stderrlog::LogLevelNum;

/*
fn pump(reader: std::fs::File) {
    std::io::BufReader::new(reader)
        .lines()
        .map_while(Result::ok)
        .for_each(|_str| {
            // Write log string to destination here.
            // For instance with journald:
            //  systemd::journal::print(0, &str);
        });
}

struct Journal {
    stdout_handle: std::thread::JoinHandle<()>,
    stderr_handle: std::thread::JoinHandle<()>,
}

impl Driver for Journal {
    type Error = String;

    fn new(config: Config) -> Result<Self, Self::Error> {
        let stdout = config.stdout;
        let stderr = config.stderr;

        Ok(Journal {
            stdout_handle: std::thread::spawn(|| pump(stdout)),
            stderr_handle: std::thread::spawn(|| pump(stderr)),
        })
    }

    fn wait(self) -> Result<(), Self::Error> {
        self.stdout_handle
            .join()
            .map_err(|err| format!("{:?}", err))?;
        self.stderr_handle
            .join()
            .map_err(|err| format!("{:?}", err))?;
        Ok(())
    }
}
*/

#[tokio::main]
async fn main() -> Result<()> {
    // containerd_shim_logging::run::<Journal>();
    // stderrlog::new()
    //     .verbosity(LogLevelNum::Trace)
    //     .init()
    //     .unwrap();
    run::<Service>("io.containerd.akari.v2", None).await;
    Ok(())
}
