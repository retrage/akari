// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmStorage {
    pub r#type: String,
    pub file: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmNetwork {
    pub r#type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmSerial {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmSharedDirectory {
    pub path: PathBuf,
    pub automount: bool,
    pub read_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmDisplay {
    pub dpi: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosVmConfig {
    pub version: usize,
    pub serial: Option<MacosVmSerial>,
    pub os: String,
    pub hardware_model: String,
    pub machine_id: String,
    pub cpus: usize,
    pub ram: usize,
    pub storage: Vec<MacosVmStorage>,
    pub networks: Vec<MacosVmNetwork>,
    pub shares: Option<Vec<MacosVmSharedDirectory>>,
    pub displays: Vec<MacosVmDisplay>,
    pub audio: bool,
}

pub fn load_vm_config(path: &Path) -> Result<MacosVmConfig> {
    let json_string = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json_string)?)
}
