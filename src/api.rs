// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::io::Write;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::vmm::config::MacosVmConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Create,
    Delete,
    Kill,
    Start,
    State,
}

impl Command {
    pub fn send(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(serde_json::to_string(self)?.as_bytes())?;
        writer.write_all(b"\0")?;
        writer.flush()?;
        Ok(())
    }

    pub fn recv(reader: &mut impl std::io::Read) -> Result<Self> {
        let mut buf = Vec::new();
        loop {
            let mut byte = [0];
            reader.read_exact(&mut byte)?;
            if byte[0] == b'\0' {
                break;
            }
            buf.push(byte[0]);
        }
        let response: Self = serde_json::from_slice(&buf)?;
        Ok(response)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VmStatus {
    Creating,
    Created,
    Running,
    Stopped,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub container_id: String,
    pub command: Command,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm_config: Option<MacosVmConfig>,
}

impl Request {
    pub fn send(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(serde_json::to_string(self)?.as_bytes())?;
        writer.write_all(b"\0")?;
        writer.flush()?;
        Ok(())
    }

    pub fn recv(reader: &mut impl std::io::Read) -> Result<Self> {
        let mut buf = Vec::new();
        loop {
            let mut byte = [0];
            reader.read_exact(&mut byte)?;
            if byte[0] == b'\0' {
                break;
            }
            buf.push(byte[0]);
        }
        let request: Self = serde_json::from_slice(&buf)?;
        Ok(request)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub container_id: String,
    pub status: VmStatus,
    pub pid: Option<i32>,
    pub config: MacosVmConfig,
}

impl Response {
    pub fn send(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(serde_json::to_string(self)?.as_bytes())?;
        writer.write_all(b"\0")?;
        writer.flush()?;
        Ok(())
    }

    pub fn recv(reader: &mut impl std::io::Read) -> Result<Self> {
        let mut buf = Vec::new();
        loop {
            let mut byte = [0];
            reader.read_exact(&mut byte)?;
            if byte[0] == b'\0' {
                break;
            }
            buf.push(byte[0]);
        }
        let response: Self = serde_json::from_slice(&buf)?;
        Ok(response)
    }
}
