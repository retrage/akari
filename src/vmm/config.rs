// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::Path;

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use icrate::{
    Foundation::{NSArray, NSData, NSFileHandle, NSString, NSURL},
    Virtualization::{
        VZDiskImageStorageDeviceAttachment, VZFileHandleSerialPortAttachment,
        VZMacAuxiliaryStorage, VZMacGraphicsDeviceConfiguration, VZMacGraphicsDisplayConfiguration,
        VZMacHardwareModel, VZMacMachineIdentifier, VZMacOSBootLoader, VZMacPlatformConfiguration,
        VZSharedDirectory, VZSingleDirectoryShare, VZVirtioBlockDeviceConfiguration,
        VZVirtioConsoleDeviceSerialPortConfiguration, VZVirtioFileSystemDeviceConfiguration,
        VZVirtioSocketDeviceConfiguration, VZVirtualMachineConfiguration,
    },
};
use objc2::{rc::Id, ClassType};

use super::api::MacosVmConfig;

pub struct Config {
    cpu_count: usize,
    ram_size: u64,
    platform: Id<VZMacPlatformConfiguration>,
    storages: Vec<Id<VZVirtioBlockDeviceConfiguration>>,
    consoles: Vec<Id<VZVirtioConsoleDeviceSerialPortConfiguration>>,
    shared_dirs: Vec<Id<VZVirtioFileSystemDeviceConfiguration>>,
    graphics: Option<Id<VZMacGraphicsDeviceConfiguration>>,
    socket: Option<Id<VZVirtioSocketDeviceConfiguration>>,
}

impl Config {
    pub fn new(cpu_count: usize, ram_size: u64) -> Self {
        Self {
            cpu_count,
            ram_size,
            platform: unsafe { VZMacPlatformConfiguration::new() },
            storages: Vec::new(),
            consoles: Vec::new(),
            shared_dirs: Vec::new(),
            graphics: None,
            socket: None,
        }
    }

    pub fn from_vm_config(vm_config: MacosVmConfig) -> Result<Self> {
        let hw_model = BASE64_STANDARD
            .decode(vm_config.hardware_model.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to decode hardware model: {}", e))?;
        let machine_id = BASE64_STANDARD
            .decode(vm_config.machine_id.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to decode machine id: {}", e))?;

        let mut config = Self::new(vm_config.cpus, vm_config.ram as u64);

        config.hw_model(hw_model)?.machine_id(machine_id)?;

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

        config.socket()?;

        if let Some(shared_dirs) = vm_config.shares {
            for shared_dir in shared_dirs {
                config.shared_dir(&shared_dir.path, shared_dir.read_only)?;
            }
        }

        config.graphics(2560, 1600, 200)?;

        Ok(config)
    }

    pub fn build(&mut self) -> Id<VZVirtualMachineConfiguration> {
        let boot_loader = unsafe { VZMacOSBootLoader::new() };

        let config = unsafe {
            let config = VZVirtualMachineConfiguration::new();
            config.setPlatform(&self.platform);
            config.setCPUCount(self.cpu_count);
            config.setMemorySize(self.ram_size);
            config.setBootLoader(Some(&boot_loader));

            if let Some(graphics) = &self.graphics {
                config.setGraphicsDevices(&NSArray::from_slice(&[graphics.as_super()]));
            };

            if let Some(socket) = &self.socket {
                config.setSocketDevices(&NSArray::from_slice(&[socket.as_super()]));
            }

            let storages = self
                .storages
                .iter()
                .map(|s| s.as_super())
                .collect::<Vec<_>>();
            config.setStorageDevices(&NSArray::from_slice(storages.as_slice()));

            let consoles = self
                .consoles
                .iter()
                .map(|c| c.as_super())
                .collect::<Vec<_>>();
            config.setSerialPorts(&NSArray::from_slice(consoles.as_slice()));

            let shared_dirs = self
                .shared_dirs
                .iter()
                .map(|s| s.as_super())
                .collect::<Vec<_>>();
            config.setDirectorySharingDevices(&NSArray::from_slice(shared_dirs.as_slice()));

            config
        };

        config
    }

    pub fn hw_model(&mut self, model: Vec<u8>) -> Result<&mut Self> {
        let model = NSData::from_vec(model);

        let hw_model = unsafe {
            VZMacHardwareModel::initWithDataRepresentation(VZMacHardwareModel::alloc(), &model)
                .ok_or(anyhow::anyhow!("Failed to create hardware model"))?
        };

        if unsafe { !hw_model.isSupported() } {
            return Err(anyhow::anyhow!("Hardware model is not supported"));
        }

        unsafe {
            self.platform.setHardwareModel(&hw_model);
        }

        Ok(self)
    }

    pub fn machine_id(&mut self, id: Vec<u8>) -> Result<&mut Self> {
        let id = NSData::from_vec(id);

        let machine_id = unsafe {
            VZMacMachineIdentifier::initWithDataRepresentation(VZMacMachineIdentifier::alloc(), &id)
                .ok_or(anyhow::anyhow!("Failed to create machine id"))?
        };

        unsafe {
            self.platform.setMachineIdentifier(&machine_id);
        }

        Ok(self)
    }

    pub fn aux(&mut self, path: &Path) -> Result<&mut Self> {
        let url = Self::path_to_nsurl(path)?;

        let aux =
            unsafe { VZMacAuxiliaryStorage::initWithURL(VZMacAuxiliaryStorage::alloc(), &url) };

        unsafe {
            self.platform.setAuxiliaryStorage(Some(&aux));
        }

        Ok(self)
    }

    pub fn storage(&mut self, path: &Path, read_only: bool) -> Result<&mut Self> {
        let url = Self::path_to_nsurl(path)?;

        let block_attachment = unsafe {
            VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
                VZDiskImageStorageDeviceAttachment::alloc(),
                &url,
                read_only,
            )
            .map_err(|e| anyhow::anyhow!(e))?
        };

        let storage = unsafe {
            VZVirtioBlockDeviceConfiguration::initWithAttachment(
                VZVirtioBlockDeviceConfiguration::alloc(),
                &block_attachment,
            )
        };

        self.storages.push(storage);

        Ok(self)
    }

    pub fn console(&mut self, fd: Option<i32>) -> Result<&mut Self> {
        let file_handle = match fd {
            Some(fd) => unsafe { NSFileHandle::initWithFileDescriptor(NSFileHandle::alloc(), fd) },
            None => unsafe { NSFileHandle::fileHandleWithNullDevice() },
        };

        let attachment = unsafe {
            VZFileHandleSerialPortAttachment::initWithFileHandleForReading_fileHandleForWriting(
                VZFileHandleSerialPortAttachment::alloc(),
                Some(&file_handle),
                Some(&file_handle),
            )
        };

        let serial = unsafe { VZVirtioConsoleDeviceSerialPortConfiguration::new() };
        unsafe {
            serial.setAttachment(Some(&attachment));
        }

        self.consoles.push(serial);

        Ok(self)
    }

    pub fn shared_dir(&mut self, path: &Path, read_only: bool) -> Result<&mut Self> {
        let url = Self::path_to_nsurl(path)?;

        let shared_dir = unsafe {
            VZSharedDirectory::initWithURL_readOnly(VZSharedDirectory::alloc(), &url, read_only)
        };
        let dir_share = unsafe {
            VZSingleDirectoryShare::initWithDirectory(VZSingleDirectoryShare::alloc(), &shared_dir)
        };

        let shared_dir = unsafe {
            VZVirtioFileSystemDeviceConfiguration::initWithTag(
                VZVirtioFileSystemDeviceConfiguration::alloc(),
                &VZVirtioFileSystemDeviceConfiguration::macOSGuestAutomountTag(),
            )
        };
        unsafe { shared_dir.setShare(Some(&dir_share)) };

        self.shared_dirs.push(shared_dir);

        Ok(self)
    }

    pub fn graphics(&mut self, width: usize, height: usize, dpi: usize) -> Result<&mut Self> {
        let display = unsafe {
            VZMacGraphicsDisplayConfiguration::initWithWidthInPixels_heightInPixels_pixelsPerInch(
                VZMacGraphicsDisplayConfiguration::alloc(),
                width as isize,
                height as isize,
                dpi as isize,
            )
        };

        let graphics = unsafe { VZMacGraphicsDeviceConfiguration::new() };
        unsafe { graphics.setDisplays(&NSArray::from_slice(&[display.as_ref()])) };

        self.graphics = Some(graphics);

        Ok(self)
    }

    pub fn socket(&mut self) -> Result<&mut Self> {
        let socket = unsafe { VZVirtioSocketDeviceConfiguration::new() };

        self.socket = Some(socket);

        Ok(self)
    }

    fn path_to_nsstring(path: &Path) -> Result<Id<NSString>> {
        let path = path.canonicalize().map_err(|e| anyhow::anyhow!(e))?;
        let path = path
            .to_str()
            .ok_or(anyhow::anyhow!("Failed to convert path to string"))?;
        Ok(NSString::from_str(path))
    }

    fn path_to_nsurl(path: &Path) -> Result<Id<NSURL>> {
        let path = Self::path_to_nsstring(path)?;
        Ok(unsafe { NSURL::fileURLWithPath(&path) })
    }
}
