// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::{
    fs::canonicalize,
    path::{Path, PathBuf},
};

use anyhow::Result;

// Return the root path of the runtime.
pub fn root_path(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(path) => Ok(canonicalize(path)?),
        None => {
            let mut default_root_path = PathBuf::from("/run/akari"); // FIXME: We cannot use this path
            if let Ok(home_path) = std::env::var("HOME") {
                if let Ok(home_path) = canonicalize(home_path) {
                    default_root_path = home_path.join(".akari/run");
                }
            }
            Ok(default_root_path)
        }
    }
}

// Return the path to the auxiliary socket file.
pub fn aux_sock_path(root_path: &Path, path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(|| {
        let mut default_aux_sock_path = root_path.to_path_buf();
        default_aux_sock_path.push("aux.sock");
        default_aux_sock_path
    })
}
