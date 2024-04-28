// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContainerCommand {
    Create(Box<oci_spec::runtime::Spec>),
    Delete,
    Kill,
    Start,
    State,
}
