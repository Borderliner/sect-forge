#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(version, about = "Sect Forge desktop editor")]
struct Arguments {
    #[arg(long)]
    game: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    sect_forge::desktop::run(Arguments::parse().game)
}
