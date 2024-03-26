// SPDX-License-Identifier: Apache-2.0

//! Create a new VM

use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    thread::sleep,
    time::Duration,
};

use base64::prelude::*;
use block2::StackBlock;
use icrate::{
    queue::{Queue, QueueAttribute},
    Foundation::{NSArray, NSData, NSError, NSFileHandle, NSString, NSURL},
    Virtualization::*,
};
use objc2::{ffi::NSInteger, msg_send_id, rc::Id, ClassType};
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

fn load_macos_vm_config(path: &Path) -> MacosVmConfig {
    let json_string = std::fs::read_to_string(path).unwrap();
    from_str(&json_string).unwrap()
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

    let aux_storage: Id<VZMacAuxiliaryStorage> =
        msg_send_id![VZMacAuxiliaryStorage::alloc(), initWithURL: aux_url.as_ref()];
    mac_platform.setAuxiliaryStorage(Some(&aux_storage));

    let hw_model_data = NSData::from_vec(hw_model_data);

    let hw_model: Id<VZMacHardwareModel> = msg_send_id![VZMacHardwareModel::alloc(), initWithDataRepresentation: hw_model_data.as_ref()];
    if !hw_model.isSupported() {
        panic!("Hardware model is not supported");
    }
    mac_platform.setHardwareModel(&hw_model);

    let machine_id_data = NSData::from_vec(machine_id_data);

    let machine_id: Id<VZMacMachineIdentifier> = msg_send_id![VZMacMachineIdentifier::alloc(), initWithDataRepresentation: machine_id_data.as_ref()];
    mac_platform.setMachineIdentifier(&machine_id);

    mac_platform
}

unsafe fn create_graphics_device_config() -> Id<VZMacGraphicsDeviceConfiguration> {
    let display: Id<VZMacGraphicsDisplayConfiguration> = msg_send_id![VZMacGraphicsDisplayConfiguration::alloc(), initWithWidthInPixels:1920 as NSInteger, heightInPixels:1200 as NSInteger, pixelsPerInch:80 as NSInteger];
    let graphics = VZMacGraphicsDeviceConfiguration::new();
    graphics.setDisplays(&NSArray::from_slice(&[display.as_ref()]));

    graphics
}

unsafe fn create_block_device_config(path: &Path) -> Id<VZVirtioBlockDeviceConfiguration> {
    let path = NSString::from_str(path.canonicalize().unwrap().to_str().unwrap());
    let url = NSURL::fileURLWithPath(&path);

    let block_attachment: Result<Id<VZDiskImageStorageDeviceAttachment>, Id<NSError>> = msg_send_id![VZDiskImageStorageDeviceAttachment::alloc(), initWithURL: url.as_ref(), readOnly: false, error:_];
    let block_device: Id<VZVirtioBlockDeviceConfiguration> = msg_send_id![VZVirtioBlockDeviceConfiguration::alloc(), initWithAttachment: block_attachment.unwrap().as_ref()];

    block_device
}

unsafe fn create_serial_port_config() -> Id<VZVirtioConsoleDeviceSerialPortConfiguration> {
    let file_handle_in = NSFileHandle::fileHandleWithStandardInput();
    let file_handle_out = NSFileHandle::fileHandleWithStandardOutput();
    let attachment: Id<VZFileHandleSerialPortAttachment> = msg_send_id![VZFileHandleSerialPortAttachment::alloc(), initWithFileHandleForReading: file_handle_in.as_ref(), fileHandleForWriting: file_handle_out.as_ref()];

    let serial = VZVirtioConsoleDeviceSerialPortConfiguration::new();
    serial.setAttachment(Some(attachment.as_ref()));

    serial
}

pub unsafe fn create_vm(bundle_path: &Path) -> Id<VZVirtualMachineConfiguration> {
    let macos_vm_config = load_macos_vm_config(&bundle_path.join("vm.json"));
    let mac_platform = create_mac_platform_config(&macos_vm_config);
    let disk = macos_vm_config
        .storage
        .iter()
        .find(|s| s.r#type == "disk")
        .unwrap();
    let graphics_device = create_graphics_device_config();
    let block_device = create_block_device_config(&disk.file);
    let serial_port = create_serial_port_config();

    let boot_loader = VZMacOSBootLoader::new();

    let config = VZVirtualMachineConfiguration::new();
    config.setPlatform(&mac_platform);
    config.setCPUCount(macos_vm_config.cpus.try_into().unwrap());
    config.setMemorySize(macos_vm_config.ram.try_into().unwrap());
    config.setBootLoader(Some(&boot_loader));
    config.setGraphicsDevices(&NSArray::from_slice(&[graphics_device.as_super()]));
    config.setStorageDevices(&NSArray::from_slice(&[block_device.as_super()]));
    config.setSerialPorts(&NSArray::from_slice(&[serial_port.as_super()]));

    config
}

pub unsafe fn start_vm(config: Id<VZVirtualMachineConfiguration>) {
    match config.validateWithError() {
        Ok(_) => {
            let queue = Queue::create("com.akari.vm.queue", QueueAttribute::Serial);
            let vm: Arc<RwLock<Id<VZVirtualMachine>>> = Arc::new(RwLock::new(
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
