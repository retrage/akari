// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use liboci_cli::Delete;

pub fn delete(args: Delete, root_path: PathBuf) -> std::io::Result<()> {
    println!("Delete: {}", args.container_id);
    Ok(())
}
