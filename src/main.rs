use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

use agenthome::{read_jsonc, render, tiers};

#[derive(Parser)]
#[command(about = "Render agenthome configuration", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render shared role templates, or check committed outputs without writing.
    RenderRoles {
        #[arg(long)]
        check: bool,
    },
    /// Print the effective unattended opencode configuration for a pool.
    RenderPromptTiers { pool: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Like the original scripts, resolve inputs relative to the source checkout,
    // not the caller's working directory. This binary is built from source.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match cli.command {
        Command::RenderRoles { check } => {
            let files = render::render_roles(&root).context("render roles")?;
            if check {
                let changed = render::drift(&root, &files)?;
                ensure!(
                    changed.is_empty(),
                    "out of date, run just render-roles: {}",
                    changed.join(", ")
                );
            } else {
                for path in render::write_roles(&root, &files)? {
                    println!("rendered {path}");
                }
            }
        }
        Command::RenderPromptTiers { pool } => {
            let config = tiers::render_opencode(
                &read_jsonc(&root.join("prompt-tiers.json"))?,
                &pool,
                &read_jsonc(&root.join("opencode/opencode.json"))?,
                &read_jsonc(&root.join("opencode/opencode-runner.json"))?,
            )?;
            println!("{}", agenthome::pretty_json(&config)?);
        }
    }
    Ok(())
}
