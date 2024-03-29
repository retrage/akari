// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::vmm::start;
use liboci_cli::Start;

pub fn start(args: Start, root_path: PathBuf) -> std::io::Result<()> {
    println!("Start: {}", args.container_id);

    // TODO: Create a VM on create
    let config = start::create_vm(&root_path, &args.container_id)?;

    unsafe {
        start::start_vm(config);
    }

    Ok(())
}
