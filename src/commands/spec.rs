// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use oci_spec::runtime::Spec;

pub fn spec(args: liboci_cli::Spec) -> Result<()> {
    if args.rootless {
        return Err(anyhow::anyhow!("Rootless containers are not supported"));
    }

    let mut spec = Spec::default();
    spec.set_hostname(Some("akari".to_string()));
    spec.set_linux(None);
    spec.set_mounts(None);

    let config_path = args
        .bundle
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.json");

    let config_json = serde_json::to_string_pretty(&spec)?;
    std::fs::write(config_path, config_json)?;

    Ok(())
}
