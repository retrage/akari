// SPDX-License-Identifier: Apache-2.0

mod commands;
mod vmm;

use clap::Parser;

use commands::{create, delete, kill, start, state};
use liboci_cli::{GlobalOpts, StandardCmd};

#[derive(clap::Parser)]
struct Opts {
    #[clap(flatten)]
    global: GlobalOpts,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(clap::Subcommand)]
enum SubCommand {
    #[clap(flatten)]
    Standard(Box<liboci_cli::StandardCmd>),
}

fn main() -> std::io::Result<()> {
    let opts = Opts::parse();

    let root_path = opts.global.root.unwrap();

    match opts.subcmd {
        SubCommand::Standard(cmd) => match *cmd {
            StandardCmd::Create(create) => create::create(create, root_path),
            StandardCmd::Start(start) => start::start(start, root_path),
            StandardCmd::Kill(kill) => kill::kill(kill, root_path),
            StandardCmd::Delete(delete) => delete::delete(delete, root_path),
            StandardCmd::State(state) => state::state(state, root_path),
        },
    }
}
