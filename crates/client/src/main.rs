// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

mod commands;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use containerd_shim::protos::shim::shim_ttrpc_async::TaskClient;
use liboci_cli::StandardCmd;
use ttrpc::asynchronous::Client;

use commands::{connect, create, delete, kill, spec, start, state};
use libakari::path::{aux_sock_path, root_path};

#[derive(clap::Parser, Debug)]
pub enum CommonCmd {
    Spec(liboci_cli::Spec),
    Connect(connect::Connect),
}

// The OCI Command Line Interface document doesn't define any global
// flags, but these are commonly accepted by runtimes
#[derive(clap::Parser, Debug)]
pub struct GlobalOpts {
    /// set the log file to write youki logs to (default is '/dev/stderr')
    #[clap(short, long, overrides_with("log"))]
    pub log: Option<PathBuf>,
    /// change log level to debug, but the `log-level` flag takes precedence
    #[clap(long)]
    pub debug: bool,
    /// set the log format ('text' (default), or 'json') (default: "text")
    #[clap(long)]
    pub log_format: Option<String>,
    /// root directory to store container state
    #[clap(short, long)]
    pub root: Option<PathBuf>,
    /// Enable systemd cgroup manager, rather then use the cgroupfs directly.
    #[clap(skip)]
    pub systemd_cgroup: bool,
    /// Specify the path to the VMM socket
    #[clap(short, long)]
    pub vmm_sock: Option<PathBuf>,
}

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
    #[clap(flatten)]
    Common(Box<CommonCmd>),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let opts = Opts::parse();

    let root_path = root_path(opts.global.root)?;
    let aux_sock_path = aux_sock_path(&root_path, opts.global.vmm_sock);

    let client = TaskClient::new(Client::connect(aux_sock_path.to_str().unwrap())?);

    match opts.subcmd {
        SubCommand::Standard(cmd) => match *cmd {
            StandardCmd::Create(create) => create::create(create, &client).await?,
            StandardCmd::Delete(delete) => delete::delete(delete, &client).await?,
            StandardCmd::Start(start) => start::start(start, &client).await?,
            StandardCmd::Kill(kill) => kill::kill(kill, &client).await?,
            StandardCmd::State(state) => state::state(state, &client).await?,
        },
        SubCommand::Common(cmd) => match *cmd {
            CommonCmd::Spec(spec) => spec::spec(spec)?,
            CommonCmd::Connect(connect) => connect::connect(connect, &client).await?,
        },
    };

    Ok(())
}
