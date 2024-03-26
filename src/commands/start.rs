// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::macosvm::create;
use liboci_cli::Start;

pub fn start(args: Start, root_path: PathBuf) -> std::io::Result<()> {
    println!("Start: {}", args.container_id);

    let config = unsafe { create::create_vm(&root_path) };

    unsafe {
        create::start_vm(config);
    }

    // Command::new(root_path.join("macosvm/macosvm"))
    //     .arg("-g")
    //     .arg("--pty")
    //     .arg(root_path.join("vm.json"))
    //     .spawn()?
    //     .wait()?;
    Ok(())
}
