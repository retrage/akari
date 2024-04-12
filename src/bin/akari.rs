// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2024 Akira Moroo

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use liboci_cli::StandardCmd;

use akari::{
    api,
    commands::{create, delete, kill, spec, start, state},
};
use tarpc::{serde_transport, tokio_serde::formats::Json};

#[derive(clap::Parser, Debug)]
pub enum CommonCmd {
    Spec(liboci_cli::Spec),
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
    let opts = Opts::parse();

    let root_path = match opts.global.root {
        Some(path) => std::fs::canonicalize(path)?,
        None => {
            let mut default_root_path = PathBuf::from("/run/akari"); // FIXME: We cannot use this path
            if let Ok(home_path) = std::env::var("HOME") {
                if let Ok(home_path) = std::fs::canonicalize(home_path) {
                    default_root_path = home_path.join(".akari/run");
                }
            }
            default_root_path
        }
    };

    let vmm_sock_path = opts.global.vmm_sock.unwrap_or_else(|| {
        let mut default_vmm_sock_path = root_path.clone();
        default_vmm_sock_path.push("vmm.sock");
        default_vmm_sock_path
    });

    let transport = serde_transport::unix::connect(vmm_sock_path, Json::default);
    let client = api::ApiClient::new(tarpc::client::Config::default(), transport.await?).spawn();

    match opts.subcmd {
        SubCommand::Standard(cmd) => match *cmd {
            StandardCmd::Create(create) => create::create(create, root_path, &client).await?,
            StandardCmd::Delete(delete) => delete::delete(delete, root_path, &client).await?,
            StandardCmd::Start(start) => start::start(start, root_path, &client).await?,
            StandardCmd::Kill(kill) => kill::kill(kill, root_path, &client).await?,
            StandardCmd::State(state) => state::state(state, root_path, &client).await?,
        },
        SubCommand::Common(cmd) => match *cmd {
            CommonCmd::Spec(spec) => spec::spec(spec)?,
        },
    };

    Ok(())
}
