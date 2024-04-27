// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    ops::Deref,
    os::{fd::FromRawFd, unix::net::UnixStream},
    rc::Rc,
    sync::{mpsc, Arc, RwLock},
};

use anyhow::Result;
use block2::RcBlock;
use icrate::{
    Foundation::NSError,
    Virtualization::{
        VZSocketDevice, VZVirtioSocketConnection, VZVirtualMachine, VZVirtualMachineConfiguration,
    },
};
use log::{error, info, trace};
use objc2::{msg_send, msg_send_id, rc::Id, ClassType};

use crate::queue::{Queue, QueueAttribute};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(Id<NSError>),
    #[error("Failed to start VM")]
    FailedToStartVm,
    #[error("Failed to stop VM")]
    FailedToStopVm,
    #[error(transparent)]
    MpscRecv(#[from] mpsc::RecvError),
    #[error("Lock poisoned")]
    LockPoisoned,
    #[error("Invalid vsock port")]
    InvalidVsockPort,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

type VsockQueues = HashMap<u32, (VecDeque<Vec<u8>>, VecDeque<Vec<u8>>)>;
pub struct Vm {
    vm: Rc<RwLock<Id<VZVirtualMachine>>>,
    queue: Queue,
    vsock_queues: Arc<RwLock<VsockQueues>>,
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
        let vm = Vm {
            vm,
            queue,
            vsock_queues: Arc::new(RwLock::new(HashMap::new())),
        };
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
            Err(e) => Err(e),
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
            Err(e) => Err(e),
        }
    }

    pub fn connect(&mut self, port: u32) -> Result<(), Error> {
        info!("Connecting VM at port {}", port);

        {
            let vsock_queue = self.vsock_queues.read().map_err(|_| Error::LockPoisoned)?;
            if vsock_queue.contains_key(&port) {
                return Err(Error::InvalidVsockPort);
            }
        }

        let (tx, rx) = mpsc::channel::<Result<(), Error>>();
        let vm = self.vm.clone();
        let vsock_queue = self.vsock_queues.clone();
        let block = RcBlock::new(move || {
            let tx = tx.clone();
            let err_tx = tx.clone();
            let vsock_queue = vsock_queue.clone();
            let completion_handler = RcBlock::new(
                move |connection: *mut VZVirtioSocketConnection, error: *mut NSError| {
                    info!("Connected to VM: {:?}", connection);
                    if connection.is_null() {
                        if !error.is_null() {
                            unsafe {
                                info!("error: {:?}", error.as_ref().unwrap());
                            }
                        }
                        err_tx
                            .send(Err(Error::FailedToStartVm))
                            .expect("Failed to send");
                        return;
                    }
                    let connection =
                        unsafe { connection.as_ref().expect("Failed to get connection") };
                    let fd = unsafe { connection.fileDescriptor() };
                    info!("fileDescriptor: {}", fd);
                    unsafe {
                        info!("sourcePort: {}", connection.sourcePort());
                        info!("destinationPort: {}", connection.destinationPort());
                    }
                    let mut stream = unsafe { UnixStream::from_raw_fd(fd) };
                    let result = Self::vsock_handler(&mut stream, port, vsock_queue.clone());
                    err_tx.send(result).expect("Failed to send");
                },
            );

            match vm.write() {
                Ok(vm) => unsafe {
                    let socket = vm.socketDevices().firstObject().unwrap();
                    Self::do_connect(socket, port, completion_handler);
                    tx.send(Ok(())).expect("Failed to send");
                },
                Err(_) => tx.send(Err(Error::LockPoisoned)).expect("Failed to send"),
            }
        });
        self.queue.exec_block_async(&block);

        match rx.recv()? {
            Ok(()) => {
                info!("VM connected");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    unsafe fn do_connect(
        socket: Id<VZSocketDevice>,
        port: u32,
        completion_handler: RcBlock<dyn Fn(*mut VZVirtioSocketConnection, *mut NSError)>,
    ) {
        let _: () = msg_send![socket.as_super(), connectToPort: port, completionHandler: completion_handler.deref()];
    }

    fn vsock_handler(
        stream: &mut UnixStream,
        port: u32,
        vsock_queues: Arc<RwLock<VsockQueues>>,
    ) -> Result<(), Error> {
        info!("vsock_handler: port={}", port);
        loop {
            {
                let mut vsock_queue = vsock_queues.write().map_err(|_| Error::LockPoisoned)?;
                let (txq, _) = vsock_queue.get_mut(&port).ok_or(Error::InvalidVsockPort)?;
                if let Some(data) = txq.pop_front() {
                    match stream.write_all(&data) {
                        Ok(_) => {}
                        Err(e) => {
                            return Err(Error::Io(e));
                        }
                    }
                }
            }

            {
                let mut buf = [0_u8; 1024];
                match stream.read(&mut buf) {
                    Ok(n) => {
                        trace!("read {} bytes", n);
                        let data = &buf[..n];
                        let mut vsock_queue =
                            vsock_queues.write().map_err(|_| Error::LockPoisoned)?;
                        let (_, rxq) = vsock_queue.get_mut(&port).ok_or(Error::InvalidVsockPort)?;
                        rxq.push_back(data.to_vec());
                    }
                    Err(e) => {
                        return Err(Error::Io(e));
                    }
                }
            }
        }

        #[allow(unreachable_code)]
        Ok(())
    }

    pub fn disconnect(&mut self, port: u32) -> Result<(), Error> {
        info!("Disconnecting VM at port {}", port);
        let mut vsock_queue = self.vsock_queues.write().map_err(|_| Error::LockPoisoned)?;
        match vsock_queue.remove(&port) {
            Some(_) => Ok(()),
            None => Err(Error::InvalidVsockPort),
        }
    }

    pub fn vsock_send(&mut self, port: u32, data: Vec<u8>) -> Result<(), Error> {
        info!("Send data to VM at port {}", port);
        let mut vsock_queue = self.vsock_queues.write().map_err(|_| Error::LockPoisoned)?;
        match vsock_queue.get_mut(&port) {
            Some((txq, _)) => {
                txq.push_back(data);
                Ok(())
            }
            None => Err(Error::InvalidVsockPort),
        }
    }

    pub fn vsock_recv(&mut self, port: u32, data: &mut Vec<u8>) -> Result<(), Error> {
        info!("Receive data from VM at port {}", port);
        let mut vsock_queue = self.vsock_queues.write().map_err(|_| Error::LockPoisoned)?;
        match vsock_queue.get_mut(&port) {
            Some((_, rxq)) => match rxq.pop_front() {
                Some(d) => {
                    data.clear();
                    data.extend_from_slice(&d);
                    Ok(())
                }
                None => Err(Error::MpscRecv(mpsc::RecvError)),
            },
            None => Err(Error::InvalidVsockPort),
        }
    }
}
