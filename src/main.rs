use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use agenthome::eval;

#[derive(Parser)]
#[command(about = "Summarize offline agenthome evaluations", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Summarize offline evaluation runs against their frozen manifest.
    SummarizeEval { manifest: PathBuf, runs: PathBuf },
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::SummarizeEval { manifest, runs } => {
            let summary = eval::summarize(&eval::read_json(&manifest)?, &eval::read_json(&runs)?)
                .context("eval")?;
            println!("{}", agenthome::pretty_json(&summary)?);
        }
    }
    Ok(ExitCode::SUCCESS)
}
