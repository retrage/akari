// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use oci_spec::runtime::Process;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum Request {
    Create(Process),
    Start(Process),
}
