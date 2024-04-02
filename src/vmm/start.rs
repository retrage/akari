// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::Path,
    sync::{mpsc, Arc, RwLock},
};

use anyhow::Result;
use block2::RcBlock;
use icrate::{
    queue::{Queue, QueueAttribute},
    Foundation::{NSArray, NSData, NSError, NSFileHandle, NSString, NSURL},
    Virtualization::*,
};
use objc2::{msg_send_id, rc::Id, ClassType};

use base64::prelude::*;

use super::config::MacosVmConfig;

unsafe fn create_mac_platform_config(
    vm_config: &MacosVmConfig,
) -> Result<Id<VZMacPlatformConfiguration>> {
    let mac_platform = VZMacPlatformConfiguration::new();

    let hw_model_data = BASE64_STANDARD
        .decode(vm_config.hardware_model.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to decode hardware model: {}", e))?;
    let machine_id_data = BASE64_STANDARD
        .decode(vm_config.machine_id.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to decode machine id: {}", e))?;

    let aux = vm_config
        .storage
        .iter()
        .find(|s| s.r#type == "aux")
        .ok_or(anyhow::anyhow!("Auxiliary storage not found"))?;

    let aux_path = NSString::from_str(
        aux.file
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to canonicalize auxiliary storage path: {}", e))?
            .to_str()
            .ok_or(anyhow::anyhow!(
                "Failed to convert auxiliary storage path to string"
            ))?,
    );
    let aux_url = NSURL::fileURLWithPath(&aux_path);

    let aux_storage = VZMacAuxiliaryStorage::initWithURL(VZMacAuxiliaryStorage::alloc(), &aux_url);
    mac_platform.setAuxiliaryStorage(Some(&aux_storage));

    let hw_model_data = NSData::from_vec(hw_model_data);

    let hw_model =
        VZMacHardwareModel::initWithDataRepresentation(VZMacHardwareModel::alloc(), &hw_model_data)
            .ok_or(anyhow::anyhow!("Failed to create hardware model"))?;
    if !hw_model.isSupported() {
        return Err(anyhow::anyhow!("Hardware model is not supported"));
    }
    mac_platform.setHardwareModel(&hw_model);

    let machine_id_data = NSData::from_vec(machine_id_data);

    let machine_id = VZMacMachineIdentifier::initWithDataRepresentation(
        VZMacMachineIdentifier::alloc(),
        &machine_id_data,
    )
    .ok_or(anyhow::anyhow!("Failed to create machine id"))?;
    mac_platform.setMachineIdentifier(&machine_id);

    Ok(mac_platform)
}

unsafe fn create_graphics_device_config() -> Id<VZMacGraphicsDeviceConfiguration> {
    let display =
        VZMacGraphicsDisplayConfiguration::initWithWidthInPixels_heightInPixels_pixelsPerInch(
            VZMacGraphicsDisplayConfiguration::alloc(),
            1920,
            1200,
            80,
        );
    let graphics = VZMacGraphicsDeviceConfiguration::new();
    graphics.setDisplays(&NSArray::from_slice(&[display.as_ref()]));

    graphics
}

unsafe fn create_block_device_config(path: &Path) -> Result<Id<VZVirtioBlockDeviceConfiguration>> {
    let path = NSString::from_str(
        path.canonicalize()
            .map_err(|e| anyhow::anyhow!(e))?
            .to_str()
            .ok_or(anyhow::anyhow!(
                "Failed to convert block device path to string"
            ))?,
    );
    let url = NSURL::fileURLWithPath(&path);

    let block_attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
        VZDiskImageStorageDeviceAttachment::alloc(),
        &url,
        false,
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    Ok(VZVirtioBlockDeviceConfiguration::initWithAttachment(
        VZVirtioBlockDeviceConfiguration::alloc(),
        &block_attachment,
    ))
}

#[allow(dead_code)]
unsafe fn create_stdio_serial_port_config(
) -> Result<Id<VZVirtioConsoleDeviceSerialPortConfiguration>> {
    let file_handle_in = NSFileHandle::fileHandleWithStandardInput();
    let file_handle_out = NSFileHandle::fileHandleWithStandardOutput();
    let attachment =
        VZFileHandleSerialPortAttachment::initWithFileHandleForReading_fileHandleForWriting(
            VZFileHandleSerialPortAttachment::alloc(),
            Some(&file_handle_in),
            Some(&file_handle_out),
        );

    let serial = VZVirtioConsoleDeviceSerialPortConfiguration::new();
    serial.setAttachment(Some(attachment.as_ref()));

    Ok(serial)
}

unsafe fn create_fd_serial_port_config(
    fd: Option<i32>,
) -> Result<Id<VZVirtioConsoleDeviceSerialPortConfiguration>> {
    let file_handle = match fd {
        Some(fd) => NSFileHandle::initWithFileDescriptor(NSFileHandle::alloc(), fd),
        None => NSFileHandle::fileHandleWithNullDevice(),
    };
    let attachment =
        VZFileHandleSerialPortAttachment::initWithFileHandleForReading_fileHandleForWriting(
            VZFileHandleSerialPortAttachment::alloc(),
            Some(&file_handle),
            Some(&file_handle),
        );

    let serial = VZVirtioConsoleDeviceSerialPortConfiguration::new();
    serial.setAttachment(Some(&attachment));

    Ok(serial)
}

unsafe fn create_directory_share_device_config(
    path: &Path,
    readonly: bool,
) -> Result<Id<VZVirtioFileSystemDeviceConfiguration>> {
    let path = NSString::from_str(
        path.canonicalize()
            .map_err(|e| anyhow::anyhow!(e))?
            .to_str()
            .ok_or(anyhow::anyhow!(
                "Failed to convert shared directory path to string"
            ))?,
    );
    let url = NSURL::fileURLWithPath(&path);

    let shared_directory =
        VZSharedDirectory::initWithURL_readOnly(VZSharedDirectory::alloc(), &url, readonly);
    let single_directory_share = VZSingleDirectoryShare::initWithDirectory(
        VZSingleDirectoryShare::alloc(),
        &shared_directory,
    );

    let sharing_config = VZVirtioFileSystemDeviceConfiguration::initWithTag(
        VZVirtioFileSystemDeviceConfiguration::alloc(),
        &VZVirtioFileSystemDeviceConfiguration::macOSGuestAutomountTag(),
    );
    sharing_config.setShare(Some(&single_directory_share));

    Ok(sharing_config)
}

pub fn create_vm(
    vm_config: MacosVmConfig,
    serial: &Option<UnixStream>,
) -> Result<Id<VZVirtualMachineConfiguration>> {
    let mac_platform = unsafe { create_mac_platform_config(&vm_config)? };

    let disk = vm_config
        .storage
        .iter()
        .find(|s| s.r#type == "disk")
        .ok_or(anyhow::anyhow!("Disk image not found"))?;
    let block_device = unsafe { create_block_device_config(&disk.file)? };

    let fd = serial.as_ref().map(|sock| sock.as_raw_fd());
    let serial_port = unsafe { create_fd_serial_port_config(fd)? };

    let shares = vm_config
        .shares
        .ok_or(anyhow::anyhow!("Shared directory not found"))?;
    let shared = shares
        .first()
        .ok_or(anyhow::anyhow!("Shared directory not found"))?;
    let directory_share =
        unsafe { create_directory_share_device_config(&shared.path, shared.read_only)? };

    let graphics_device = unsafe { create_graphics_device_config() };

    let boot_loader = unsafe { VZMacOSBootLoader::new() };

    let config = unsafe {
        let config = VZVirtualMachineConfiguration::new();
        config.setPlatform(&mac_platform);
        config.setCPUCount(vm_config.cpus);
        config.setMemorySize(
            vm_config
                .ram
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid RAM size"))?,
        );
        config.setBootLoader(Some(&boot_loader));
        config.setGraphicsDevices(&NSArray::from_slice(&[graphics_device.as_super()]));
        config.setStorageDevices(&NSArray::from_slice(&[block_device.as_super()]));
        config.setSerialPorts(&NSArray::from_slice(&[serial_port.as_super()]));
        config.setDirectorySharingDevices(&NSArray::from_slice(&[directory_share.as_super()]));
        config
    };

    Ok(config)
}

pub struct Vm {
    vm: Arc<RwLock<Id<VZVirtualMachine>>>,
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
        let vm: Arc<RwLock<Id<VZVirtualMachine>>> = Arc::new(RwLock::new(unsafe {
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
