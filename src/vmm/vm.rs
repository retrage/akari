// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{
    os::{fd::AsRawFd, unix::net::UnixStream},
    rc::Rc,
    sync::{mpsc, RwLock},
};

use anyhow::Result;
use block2::RcBlock;
use icrate::{
    queue::{Queue, QueueAttribute},
    Foundation::NSError,
    Virtualization::*,
};
use objc2::{msg_send_id, rc::Id, ClassType};

use base64::prelude::*;

use super::{api::MacosVmConfig, config::Config};

pub fn create_config(
    vm_config: MacosVmConfig,
    console: &Option<UnixStream>,
) -> Result<Id<VZVirtualMachineConfiguration>> {
    let hw_model = BASE64_STANDARD
        .decode(vm_config.hardware_model.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to decode hardware model: {}", e))?;
    let machine_id = BASE64_STANDARD
        .decode(vm_config.machine_id.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to decode machine id: {}", e))?;

    let mut config = Config::new(vm_config.cpus, vm_config.ram as u64);

    config.hw_model(hw_model)?.machine_id(machine_id)?;
    config.console(console.as_ref().map(|s| s.as_raw_fd()))?;

    for storage in vm_config.storage {
        match storage.r#type.as_str() {
            "disk" => {
                config.storage(&storage.file, false)?;
            }
            "aux" => {
                config.aux(&storage.file)?;
            }
            _ => {}
        }
    }

    if let Some(shared_dirs) = vm_config.shares {
        for shared_dir in shared_dirs {
            config.shared_dir(&shared_dir.path, shared_dir.read_only)?;
        }
    }

    config.graphics(2560, 1600, 200)?;

    Ok(config.build())
}

pub struct Vm {
    vm: Rc<RwLock<Id<VZVirtualMachine>>>,
    queue: Queue,
}

impl Vm {
    pub fn new(config: Id<VZVirtualMachineConfiguration>) -> Result<Self> {
        unsafe {
            if let Err(e) = config.validateWithError() {
                return Err(anyhow::anyhow!(e));
            }
        }
        let queue = Queue::create("com.akari.vm.queue", QueueAttribute::Serial);
        let vm: Rc<RwLock<Id<VZVirtualMachine>>> = Rc::new(RwLock::new(unsafe {
            msg_send_id![VZVirtualMachine::alloc(), initWithConfiguration: config.as_ref(), queue: queue.ptr]
        }));
        let vm = Vm { vm, queue };
        Ok(vm)
    }

    pub fn start(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel::<Result<()>>();
        let vm_clone = self.vm.clone();
        let dispatch_block = RcBlock::new(move || {
            let inner_tx = tx.clone();
            let completion_handler = RcBlock::new(move |error: *mut NSError| {
                if !error.is_null() {
                    inner_tx
                        .send(Err(anyhow::anyhow!("Failed to start VM")))
                        .expect("Failed to send error");
                } else {
                    inner_tx.send(Ok(())).expect("Failed to send result");
                }
            });
            unsafe {
                vm_clone
                    .write()
                    .unwrap()
                    .startWithCompletionHandler(&completion_handler);
            }
        });
        self.queue.exec_block_async(&dispatch_block);

        rx.recv()?
    }

    pub fn kill(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel::<Result<()>>();
        let vm_clone = self.vm.clone();
        let dispatch_block = RcBlock::new(move || {
            let inner_tx = tx.clone();
            let completion_handler = RcBlock::new(move |error: *mut NSError| {
                if !error.is_null() {
                    inner_tx
                        .send(Err(anyhow::anyhow!("Failed to stop VM")))
                        .expect("Failed to send error");
                } else {
                    inner_tx.send(Ok(())).expect("Failed to send result");
                }
            });
            unsafe {
                if vm_clone.read().expect("Failed to read lock").canStop() {
                    vm_clone
                        .write()
                        .expect("Failed to write lock")
                        .stopWithCompletionHandler(&completion_handler);
                } else {
                    panic!("VM cannot be stopped");
                }
            }
        });
        self.queue.exec_block_async(&dispatch_block);

        rx.recv()?
    }
}
