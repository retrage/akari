// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! Akari Guest Agent
//! This is a daemon that listens for requests from the host.

use std::{
    collections::HashMap,
    io::Read,
    process::{Command, Stdio},
};

use anyhow::Result;
use libakari::agent_api::Request;
use oci_spec::runtime::Process;
use vsock::{VsockAddr, VsockListener, VMADDR_CID_ANY};

fn start(process: Process) -> Result<()> {
    let cwd = process.cwd();
    let args = process.args().as_ref().unwrap();
    let env = process.env();

    assert!(!args.is_empty());
    let cmd = args[0].clone();
    let args = &args[1..];

    let mut cmd = Command::new(cmd);
    cmd.current_dir(cwd);
    cmd.args(args);
    if let Some(env) = env {
        // Create hashmap by parsing env strings like "key=value"
        let envs: HashMap<String, String> = env
            .iter()
            .map(|e| {
                let mut split = e.splitn(2, '=');
                (
                    split.next().unwrap().to_string(),
                    split.next().unwrap().to_string(),
                )
            })
            .collect();
        cmd.envs(envs);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::piped());

    Ok(())
}

fn handle_request(request: Request) -> Result<()> {
    match request {
        Request::Create(process) => {
            log::info!("Creating process: {:?}", process);
            start(process)
        }
        Request::Start(process) => {
            log::info!("Starting process: {:?}", process);
            start(process)
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();

    let addr = VsockAddr::new(VMADDR_CID_ANY, 9999);
    let listener = VsockListener::bind(&addr)?;

    for stream in listener.incoming() {
        let mut stream = stream?;
        log::info!("Accepted a new connection from {}", stream.peer_addr()?);

        let mut buf = [0; 1024];
        let n = stream.read(&mut buf)?;
        let request = serde_json::from_slice(&buf[..n])?;
        handle_request(request)?;
    }

    Ok(())
}
