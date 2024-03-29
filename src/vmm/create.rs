// SPDX-License-Identifier: Apache-2.0

//! Create a new VM

use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::RwLock,
    thread::sleep,
    time::Duration,
};

use block2::StackBlock;
use icrate::{
    queue::{Queue, QueueAttribute},
    Foundation::{NSArray, NSData, NSError, NSFileHandle, NSString, NSURL},
    Virtualization::*,
};
use objc2::{rc::Id, ClassType};

use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::from_str;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacosVmStorage {
    r#type: String,
    file: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacosVmNetwork {
    r#type: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacosVmDisplay {
    dpi: usize,
    width: usize,
    height: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacosVmConfig {
    version: usize,
    serial: bool,
    os: String,
    hardware_model: String,
    machine_id: String,
    cpus: usize,
    ram: usize,
    storage: Vec<MacosVmStorage>,
    networks: Vec<MacosVmNetwork>,
    displays: Vec<MacosVmDisplay>,
    audio: bool,
}

fn load_macos_vm_config(path: &Path) -> Result<MacosVmConfig, std::io::Error> {
    let json_string = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json_string)?)
}

unsafe fn create_mac_platform_config(vm_config: &MacosVmConfig) -> Id<VZMacPlatformConfiguration> {
    let mac_platform = VZMacPlatformConfiguration::new();

    let hw_model_data = BASE64_STANDARD
        .decode(vm_config.hardware_model.as_bytes())
        .unwrap();
    let machine_id_data = BASE64_STANDARD
        .decode(vm_config.machine_id.as_bytes())
        .unwrap();

    let aux = vm_config
        .storage
        .iter()
        .find(|s| s.r#type == "aux")
        .unwrap();

    let aux_path = NSString::from_str(aux.file.canonicalize().unwrap().to_str().unwrap());
    let aux_url = NSURL::fileURLWithPath(&aux_path);

    let aux_storage = VZMacAuxiliaryStorage::initWithURL(VZMacAuxiliaryStorage::alloc(), &aux_url);
    mac_platform.setAuxiliaryStorage(Some(&aux_storage));

    let hw_model_data = NSData::from_vec(hw_model_data);

    let hw_model =
        VZMacHardwareModel::initWithDataRepresentation(VZMacHardwareModel::alloc(), &hw_model_data)
            .unwrap();
    if !hw_model.isSupported() {
        panic!("Hardware model is not supported");
    }
    mac_platform.setHardwareModel(&hw_model);

    let machine_id_data = NSData::from_vec(machine_id_data);

    let machine_id = VZMacMachineIdentifier::initWithDataRepresentation(
        VZMacMachineIdentifier::alloc(),
        &machine_id_data,
    )
    .unwrap();
    mac_platform.setMachineIdentifier(&machine_id);

    mac_platform
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

unsafe fn create_block_device_config(path: &Path) -> Id<VZVirtioBlockDeviceConfiguration> {
    let path = NSString::from_str(path.canonicalize().unwrap().to_str().unwrap());
    let url = NSURL::fileURLWithPath(&path);

    let block_attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
        VZDiskImageStorageDeviceAttachment::alloc(),
        &url,
        false,
    )
    .unwrap();

    VZVirtioBlockDeviceConfiguration::initWithAttachment(
        VZVirtioBlockDeviceConfiguration::alloc(),
        &block_attachment,
    )
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
    _tag: &str,
    readonly: bool,
) -> Id<VZVirtioFileSystemDeviceConfiguration> {
    let path = NSString::from_str(path.canonicalize().unwrap().to_str().unwrap());
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

    sharing_config
}

pub unsafe fn create_vm(
    bundle_path: &Path,
    container_id: &str,
) -> Id<VZVirtualMachineConfiguration> {
    let macos_vm_config = load_macos_vm_config(&bundle_path.join("vm.json")).unwrap();
    let mac_platform = create_mac_platform_config(&macos_vm_config);
    let disk = macos_vm_config
        .storage
        .iter()
        .find(|s| s.r#type == "disk")
        .unwrap();
    let graphics_device = create_graphics_device_config();
    let block_device = create_block_device_config(&disk.file);
    let serial_port = create_serial_port_config();
    let directory_share =
        create_directory_share_device_config(&bundle_path.join("shared"), container_id, false);

    let boot_loader = VZMacOSBootLoader::new();

    let config = VZVirtualMachineConfiguration::new();
    config.setPlatform(&mac_platform);
    config.setCPUCount(macos_vm_config.cpus);
    config.setMemorySize(macos_vm_config.ram.try_into().unwrap());
    config.setBootLoader(Some(&boot_loader));
    config.setGraphicsDevices(&NSArray::from_slice(&[graphics_device.as_super()]));
    config.setStorageDevices(&NSArray::from_slice(&[block_device.as_super()]));
    config.setSerialPorts(&NSArray::from_slice(&[serial_port.as_super()]));
    config.setDirectorySharingDevices(&NSArray::from_slice(&[directory_share.as_super()]));

    config
}

pub unsafe fn start_vm(config: Id<VZVirtualMachineConfiguration>) {
    match config.validateWithError() {
        Ok(_) => {
            let queue = Queue::create("com.akari.vm.queue", QueueAttribute::Serial);
            let vm: Rc<RwLock<Id<VZVirtualMachine>>> =
                Rc::new(RwLock::new(VZVirtualMachine::initWithConfiguration_queue(
                    VZVirtualMachine::alloc(),
                    &config,
                    &queue.ptr,
                )));
            let dispatch_block = StackBlock::new(move || {
                let completion_handler = StackBlock::new(|error: *mut NSError| {
                    if !error.is_null() {
                        println!("error: {:?}", error);
                    }
                });
                let completion_handler = completion_handler.copy();
                vm.write()
                    .unwrap()
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
