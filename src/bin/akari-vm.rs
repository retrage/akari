// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

//! # Akari Virtual Machine
//! This is a daemon that listens for requests from the akari OCI runtime to manage containers.

use std::{
    collections::HashMap,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::mpsc,
};

use anyhow::Result;
use clap::Parser;

use akari::{api, vmm};
use std::thread;

#[derive(clap::Parser)]
struct Opts {
    socket: PathBuf,
}

struct VmState {
    config: vmm::config::MacosVmConfig,
    status: api::VmStatus,
}

type VmStateMap = HashMap<String, VmState>;

fn create(
    state_map: &mut VmStateMap,
    request: api::Request,
) -> Result<thread::JoinHandle<Result<()>>> {
    let vm_config = match request.vm_config {
        Some(config) => config,
        None => return Err(anyhow::anyhow!("No VM config provided")),
    };

    if state_map.contains_key(&request.container_id) {
        return Err(anyhow::anyhow!("Container already exists"));
    }

    let state = VmState {
        config: vm_config.clone(),
        status: api::VmStatus::Creating,
    };

    state_map.insert(request.container_id.clone(), state);

    let (tx, rx) = mpsc::channel::<api::VmStatus>();
    let thread = thread::spawn(move || -> Result<()> {
        let serial_sock = match &vm_config.serial {
            Some(serial) => Some(UnixStream::connect(&serial.path)?),
            None => None,
        };

        let config = vmm::start::create_vm(vm_config, &serial_sock)?;
        let vm = vmm::start::Vm::new(config.clone())?;
        tx.send(api::VmStatus::Created)?;

        let cmd_listener = UnixListener::bind("/tmp/cmd.sock")?;
        for stream in cmd_listener.incoming() {
            let mut stream = stream?;
            let cmd = api::Command::recv(&mut stream)?;
            match cmd {
                api::Command::Start => vm.start()?,
                api::Command::Kill => vm.kill()?,
                _ => todo!(),
            }
        }
        Ok(())
    });

    state_map
        .get_mut(&request.container_id)
        .ok_or(anyhow::anyhow!("Container not found"))?
        .status = rx.recv()?;

    Ok(thread)
}

fn kill(state_map: &mut VmStateMap, request: api::Request) -> Result<()> {
    let state = state_map
        .get_mut(&request.container_id)
        .ok_or(anyhow::anyhow!("Container not found"))?;
    match state.status {
        api::VmStatus::Created | api::VmStatus::Running => {
            let mut cmd_sock = UnixStream::connect("/tmp/cmd.sock")?;
            let cmd = api::Command::Kill;
            cmd.send(&mut cmd_sock)?;

            state.status = api::VmStatus::Stopped;
        }
        api::VmStatus::Stopped => return Err(anyhow::anyhow!("Container already stopped")),
        _ => return Err(anyhow::anyhow!("Container not created")),
    }

    Ok(())
}

fn start(state_map: &mut VmStateMap, request: api::Request) -> Result<()> {
    let state = state_map
        .get_mut(&request.container_id)
        .ok_or(anyhow::anyhow!("Container not found"))?;
    match state.status {
        api::VmStatus::Created => {
            let mut cmd_sock = UnixStream::connect("/tmp/cmd.sock")?;
            let cmd = api::Command::Start;
            cmd.send(&mut cmd_sock)?;

            state.status = api::VmStatus::Running;
        }
        api::VmStatus::Running => return Err(anyhow::anyhow!("Container already running")),
        _ => return Err(anyhow::anyhow!("Container not created")),
    }

    Ok(())
}

fn state(stream: &mut UnixStream, state_map: &VmStateMap, request: api::Request) -> Result<()> {
    let state = state_map
        .get(&request.container_id)
        .ok_or(anyhow::anyhow!("Container not found"))?;

    let response = api::Response {
        container_id: request.container_id,
        status: state.status.clone(),
        pid: None,
        config: state.config.clone(),
    };

    response.send(stream)?;

    Ok(())
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    let mut state_map = VmStateMap::new();

    let listener = UnixListener::bind(opts.socket)?;

    let mut threads = Vec::new();

    for stream in listener.incoming() {
        let mut stream = stream?;
        let request = api::Request::recv(&mut stream)?;
        match request.command {
            api::Command::Create => {
                let thread = create(&mut state_map, request)?;
                threads.push(thread);
            }
            api::Command::Delete => todo!(),
            api::Command::Kill => kill(&mut state_map, request)?,
            api::Command::Start => start(&mut state_map, request)?,
            api::Command::State => state(&mut stream, &state_map, request)?,
        }
    }

    Ok(())
}
