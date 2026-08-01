mod cli;
mod commands;
mod config;
mod hook;
mod secret;
mod store;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() {
    disable_core_dumps();
    if let Err(e) = run() {
        eprintln!("reliquary: error: {e:#}");
        std::process::exit(1);
    }
}

/// Best-effort: a crash shouldn't leave secrets sitting in a core dump on disk.
fn disable_core_dumps() {
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &limit);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => commands::init()?,
        Command::Add { name } => commands::add(&name)?,
        Command::Remove { name } => commands::remove(&name)?,
        Command::List => commands::list()?,
        Command::Hook { shell } => hook::run(shell),
    }
    Ok(())
}
