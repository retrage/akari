// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::io::{Read, Write};

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub trait WriteTo {
    fn send(&self, writer: &mut impl Write) -> Result<()>
    where
        Self: Serialize,
    {
        writer.write_all(serde_json::to_string(self)?.as_bytes())?;
        writer.write_all(b"\0")?;
        writer.flush()?;
        Ok(())
    }
}

pub trait ReadFrom {
    fn recv(reader: &mut impl Read) -> Result<Self>
    where
        Self: Sized + for<'de> Deserialize<'de>,
    {
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
