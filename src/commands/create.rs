// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use liboci_cli::Create;

pub fn create(args: Create, root_path: PathBuf) -> std::io::Result<()> {
    println!("create: {:?}", args.bundle);
    if !root_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "root path does not exist",
        ));
    }
    if !args.bundle.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "bundle path does not exist",
        ));
    }
    // Generate a VM configuration file from the bundle
    Ok(())
}
