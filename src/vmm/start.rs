// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{path::Path, rc::Rc, sync::RwLock, thread::sleep, time::Duration};

use anyhow::Result;
use block2::StackBlock;
use icrate::{
    queue::{Queue, QueueAttribute},
    Foundation::{NSArray, NSData, NSError, NSFileHandle, NSString, NSURL},
    Virtualization::*,
};
use objc2::{msg_send_id, rc::Id, ClassType};

use base64::prelude::*;

use super::config::{load_vm_config, MacosVmConfig};

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

unsafe fn create_serial_port_config() -> Id<VZVirtioConsoleDeviceSerialPortConfiguration> {
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

    serial
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
    root_path: &Path,
    container_id: &str,
) -> Result<Id<VZVirtualMachineConfiguration>> {
    let config_path = root_path.join(format!("{}.json", container_id));

    let macos_vm_config = load_vm_config(&config_path)?;
    let mac_platform = unsafe { create_mac_platform_config(&macos_vm_config)? };

    let disk = macos_vm_config
        .storage
        .iter()
        .find(|s| s.r#type == "disk")
        .ok_or(anyhow::anyhow!("Disk image not found"))?;
    let block_device = unsafe { create_block_device_config(&disk.file)? };

    let shares = macos_vm_config
        .shares
        .ok_or(anyhow::anyhow!("Shared directory not found"))?;
    let shared = shares
        .first()
        .ok_or(anyhow::anyhow!("Shared directory not found"))?;
    let directory_share =
        unsafe { create_directory_share_device_config(&shared.path, shared.automount)? };

    let graphics_device = unsafe { create_graphics_device_config() };
    let serial_port = unsafe { create_serial_port_config() };

    let boot_loader = unsafe { VZMacOSBootLoader::new() };

    let config = unsafe {
        let config = VZVirtualMachineConfiguration::new();
        config.setPlatform(&mac_platform);
        config.setCPUCount(macos_vm_config.cpus);
        config.setMemorySize(
            macos_vm_config
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

pub unsafe fn start_vm(config: Id<VZVirtualMachineConfiguration>) {
    match config.validateWithError() {
        Ok(_) => {
            let queue = Queue::create("com.akari.vm.queue", QueueAttribute::Serial);
            let vm: Rc<RwLock<Id<VZVirtualMachine>>> = Rc::new(RwLock::new(
                msg_send_id![VZVirtualMachine::alloc(), initWithConfiguration: config.as_ref(), queue: queue.ptr],
            ));
            let dispatch_block = StackBlock::new(move || {
                let completion_handler = StackBlock::new(|error: *mut NSError| {
                    if !error.is_null() {
                        println!("error: {:?}", error);
                    }
                });
                let completion_handler = completion_handler.copy();
                vm.write()
                    .expect("Failed to lock VM")
                    .startWithCompletionHandler(&completion_handler);
            });
            let dispatch_block = dispatch_block.clone();
            queue.exec_block_async(&dispatch_block);

            sleep(Duration::from_secs(3600)); // FIXME: wait for a signal to stop
        }
        Err(e) => {
            println!("error: {:?}", e);
        }
    }
}
