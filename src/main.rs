mod angular;
mod angular_cli;
mod cli;
mod completion;
mod config;
mod error;
mod npm;
mod package;
mod platform;
mod scanner;
mod tui;
mod workspace;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run()
}