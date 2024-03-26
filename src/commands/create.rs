// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use liboci_cli::Create;

pub fn create(args: Create, _root_path: PathBuf) -> std::io::Result<()> {
    println!("create: {:?}", args.bundle);
    // Generate a VM configuration file from the bundle
    Ok(())
}
