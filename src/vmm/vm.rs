// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{
    rc::Rc,
    sync::{mpsc, RwLock},
};

use anyhow::Result;
use block2::RcBlock;
use icrate::{
    queue::{Queue, QueueAttribute},
    Foundation::NSError,
    Virtualization::{VZVirtualMachine, VZVirtualMachineConfiguration},
};
use log::info;
use objc2::{msg_send_id, rc::Id, ClassType};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(Id<NSError>),
    #[error("Failed to start VM")]
    FailedToStartVm,
    #[error("Failed to stop VM")]
    FailedToStopVm,
    #[error(transparent)]
    FailedToRecvError(#[from] mpsc::RecvError),
    #[error("Lock poisoned")]
    LockPoisoned,
}

pub struct Vm {
    vm: Rc<RwLock<Id<VZVirtualMachine>>>,
    queue: Queue,
}

impl Vm {
    pub fn new(config: Id<VZVirtualMachineConfiguration>) -> Result<Self, Error> {
        unsafe {
            config
                .validateWithError()
                .map_err(Error::InvalidConfiguration)?;
        }
        let queue = Queue::create("com.akari.vm.queue", QueueAttribute::Serial);
        let vm: Rc<RwLock<Id<VZVirtualMachine>>> = Rc::new(RwLock::new(unsafe {
            msg_send_id![VZVirtualMachine::alloc(), initWithConfiguration: config.as_ref(), queue: queue.ptr]
        }));
        let vm = Vm { vm, queue };
        Ok(vm)
    }

    pub fn start(&self) -> Result<(), Error> {
        info!("Starting VM");
        let (tx, rx) = mpsc::channel::<Result<(), Error>>();
        let vm = self.vm.clone();
        let block = RcBlock::new(move || {
            let tx = tx.clone();
            let err_tx = tx.clone();
            let completion_handler = RcBlock::new(move |error: *mut NSError| {
                if !error.is_null() {
                    err_tx
                        .send(Err(Error::FailedToStartVm))
                        .expect("Failed to send");
                } else {
                    err_tx.send(Ok(())).expect("Failed to send");
                }
            });

            match vm.write() {
                Ok(vm) => unsafe { vm.startWithCompletionHandler(&completion_handler) },
                Err(_) => tx.send(Err(Error::LockPoisoned)).expect("Failed to send"),
            }
        });
        self.queue.exec_block_async(&block);

        match rx.recv()? {
            Ok(()) => {
                info!("VM started");
                Ok(())
            }
            Err(e) => return Err(e),
        }
    }

    pub fn kill(&self) -> Result<(), Error> {
        info!("Stopping VM");
        let (tx, rx) = mpsc::channel::<Result<(), Error>>();
        let vm = self.vm.clone();
        let block = RcBlock::new(move || {
            let err_tx = tx.clone();
            let completion_handler = RcBlock::new(move |error: *mut NSError| {
                if !error.is_null() {
                    err_tx
                        .send(Err(Error::FailedToStopVm))
                        .expect("Failed to send");
                } else {
                    err_tx.send(Ok(())).expect("Failed to send");
                }
            });
            match vm.write() {
                Ok(vm) => unsafe { vm.stopWithCompletionHandler(&completion_handler) },
                Err(_) => tx.send(Err(Error::LockPoisoned)).expect("Failed to send"),
            }
        });
        self.queue.exec_block_async(&block);

        match rx.recv()? {
            Ok(()) => {
                info!("VM stopped");
                Ok(())
            }
            Err(e) => return Err(e),
        }
    }
}
