use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sect_forge::{config, export, game, portraits};

#[derive(Parser)]
#[command(
    version,
    about = "Create a custom 190 CE faction for Total War: Three Kingdoms"
)]
struct Arguments {
    /// Three Kingdoms installation directory. Steam is detected automatically.
    #[arg(long, global = true)]
    game: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Export an existing faction configuration.
    Build {
        config: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List traits and their in-game artwork.
    Traits,
    /// List available vanilla or Workshop portraits.
    Portraits {
        #[arg(long)]
        pack: Option<PathBuf>,
    },
}

fn run() -> Result<()> {
    let args = Arguments::parse();
    let path = args
        .game
        .or_else(game::discover_game)
        .context("Choose the Three Kingdoms installation with --game.")?;
    let game = game::Game::open(&path)?;
    match args.command {
        Command::Build { config, output } => {
            let bytes = fs::read(&config).with_context(|| format!("Read {}", config.display()))?;
            let config =
                serde_json::from_slice(&bytes).context("Read the faction configuration")?;
            let path = export::build(
                &game,
                config,
                &output.unwrap_or_else(config::output_dir),
                |message| eprintln!("{message}"),
            )?;
            println!("{}", path.display());
        }
        Command::Traits => {
            for item in game.traits()? {
                println!(
                    "{}\t{}\t{}",
                    item.title,
                    item.key,
                    item.icon.as_deref().unwrap_or("")
                );
            }
        }
        Command::Portraits { pack } => {
            let pack = pack.as_deref().map(game::open_pack).transpose()?;
            for item in portraits::catalog(&game.pack, pack.as_ref())? {
                println!(
                    "{}\t{}\t{}\t{}",
                    item.label,
                    item.gender,
                    item.key,
                    if item.can_export {
                        "Playable"
                    } else {
                        "Preview"
                    }
                );
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
