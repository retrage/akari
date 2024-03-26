// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use liboci_cli::Kill;

pub fn kill(args: Kill, root_path: PathBuf) -> std::io::Result<()> {
    println!("Kill: {}", args.container_id);
    Ok(())
}
