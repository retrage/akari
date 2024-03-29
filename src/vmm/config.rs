// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmStorage {
    pub r#type: String,
    pub file: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmNetwork {
    pub r#type: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmSharedDirectory {
    pub path: PathBuf,
    pub automount: bool,
    pub read_only: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmDisplay {
    pub dpi: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmConfig {
    pub version: usize,
    pub serial: bool,
    pub os: String,
    pub hardware_model: String,
    pub machine_id: String,
    pub cpus: usize,
    pub ram: usize,
    pub storage: Vec<MacosVmStorage>,
    pub networks: Vec<MacosVmNetwork>,
    pub shares: Vec<MacosVmSharedDirectory>,
    pub displays: Vec<MacosVmDisplay>,
    pub audio: bool,
}

pub fn load_vm_config(path: &Path) -> Result<MacosVmConfig, std::io::Error> {
    let json_string = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json_string)?)
}
