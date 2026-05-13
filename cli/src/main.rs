mod mikrotik_cmd;
mod network_cmd;

use anyhow::Result;
use clap::{Parser, Subcommand};
use mikrotik_cmd::subcmd::MikrotikCmd;
use network_cmd::subcmd::NetworkCmd;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "owntier", about = "owntier — WireGuard + BGP mesh management")]
struct Cli {
    /// owntier data directory (default: ~/.config/owntier)
    #[arg(long, global = true)]
    dir: Option<PathBuf>,

    /// p43 key store directory (default: ~/.config/project-43/keys)
    #[arg(long, global = true)]
    store: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Network — create and manage WireGuard + BGP mesh networks
    #[command(subcommand)]
    Network(NetworkCmd),

    /// MikroTik — attach and manage MikroTik plugin config for a network
    #[command(subcommand)]
    Mikrotik(MikrotikCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let owntier_dir = cli.dir.unwrap_or_else(|| {
        dirs::home_dir()
            .expect("cannot find home dir")
            .join(".config")
            .join("owntier")
    });

    let store_dir = cli.store.unwrap_or_else(|| {
        dirs::home_dir()
            .expect("cannot find home dir")
            .join(".config")
            .join("project-43")
            .join("keys")
    });

    match cli.command {
        Command::Network(cmd) => network_cmd::run(cmd, &owntier_dir, &store_dir),
        Command::Mikrotik(cmd) => mikrotik_cmd::run(cmd, &owntier_dir, &store_dir),
    }
}
