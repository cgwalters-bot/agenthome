use std::{env, fs, path::PathBuf, process::ExitCode, time::Duration};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

use agenthome::{
    eval,
    install::{self, Options, System},
    read_jsonc, render, tiers,
};

/// One `just install-tools` run, which itself bounds each command it starts.
const CHECK_INSTALL_TIMEOUT: Duration = Duration::from_secs(900);
const CHECK_INSTALL_OUTPUT: u64 = 1024 * 1024;

#[derive(Parser)]
#[command(about = "Render and install agenthome configuration", version)]
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
    /// Install the tools in tools.toml into $HOME/.local/bin.
    InstallTools,
    /// Install twice against the network; the second run must find every tool present.
    CheckInstall,
    /// Summarize offline evaluation runs against their frozen manifest.
    SummarizeEval { manifest: PathBuf, runs: PathBuf },
}

fn install_tools(root: PathBuf) -> Result<bool> {
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "supported platform: Linux x86_64 only"
    );
    let path = root.join("tools.toml");
    let tools = install::parse_manifest(
        &fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
    )
    .with_context(|| format!("parse {}", path.display()))?;
    let home = env::var_os("HOME").context("HOME is not set")?;
    let options = Options {
        bin: PathBuf::from(home).join(".local/bin"),
        root,
        path: env::var("PATH").unwrap_or_default(),
    };
    Ok(install::install_tools(&tools, &options, &mut System))
}

fn check_install(root: PathBuf) -> Result<()> {
    let tools = install::parse_manifest(&fs::read_to_string(root.join("tools.toml"))?)?;
    let exe = env::current_exe().context("locate this binary")?;
    let exe = exe.to_str().context("non-UTF-8 binary path")?;
    let install = || -> Result<Vec<String>> {
        let output = install::run_bounded(
            exe,
            &["install-tools".into()],
            &Vec::new(),
            CHECK_INSTALL_TIMEOUT,
            CHECK_INSTALL_OUTPUT,
        )?;
        let output = String::from_utf8(output).context("installer output is not UTF-8")?;
        print!("{output}");
        Ok(output.trim().lines().map(str::to_owned).collect())
    };
    install().context("first installation")?;
    let expected: Vec<_> = tools
        .iter()
        .filter(|tool| !tool.dependency_only)
        .map(|tool| format!("{}: present", tool.name))
        .collect();
    let second = install().context("second installation")?;
    ensure!(
        second == expected,
        "second installation must leave all tools present without reinstalling"
    );
    Ok(())
}

fn main() -> Result<ExitCode> {
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
        Command::InstallTools => {
            if !install_tools(root).context("install-tools")? {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::CheckInstall => check_install(root).context("check-install")?,
        Command::SummarizeEval { manifest, runs } => {
            let summary = eval::summarize(&eval::read_json(&manifest)?, &eval::read_json(&runs)?)
                .context("eval")?;
            println!("{}", agenthome::pretty_json(&summary)?);
        }
    }
    Ok(ExitCode::SUCCESS)
}
