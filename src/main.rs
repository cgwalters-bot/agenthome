use std::{env, fs, path::PathBuf, process::ExitCode, time::Duration};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

use agenthome::{
    eval,
    install::{self, Options, System},
};

/// One `just install-tools` run, which itself bounds each command it starts.
const CHECK_INSTALL_TIMEOUT: Duration = Duration::from_secs(900);
const CHECK_INSTALL_OUTPUT: u64 = 1024 * 1024;

#[derive(Parser)]
#[command(about = "Install agenthome tools and summarize evaluations", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install the tools in tools.toml into $HOME/.local/bin.
    InstallTools,
    /// Install twice against the network; the second run must find every tool present.
    CheckInstall,
    /// Summarize offline evaluation runs against their frozen manifest.
    SummarizeEval { manifest: PathBuf, runs: PathBuf },
    /// Check dated allocation/enforcement declarations offline; never launches inference.
    CheckEvalReadiness { evidence: PathBuf },
    /// Print an opt-in evaluation configuration pair; never installs it.
    EvalBundle {
        arm: String,
        #[arg(long, default_value = "praxis")]
        pool: String,
    },
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
        Command::CheckEvalReadiness { evidence } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let result = agenthome::readiness::check(&eval::read_json(&evidence)?, now)?;
            // Money uses exact integer microdollars, not JavaScript f64 spelling.
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::EvalBundle { arm, pool } => {
            let read = |path: &str| agenthome::read_jsonc(&root.join(path));
            let bundle = agenthome::tiers::eval_bundle(
                &read("prompt-tiers.json")?,
                &read("opencode/opencode.json")?,
                &read("opencode/opencode-runner.json")?,
                &read("roles/opencode/eval-tiered.json")?,
                &pool,
                &arm,
            )?;
            println!("{}", agenthome::pretty_json(&bundle)?);
        }
    }
    Ok(ExitCode::SUCCESS)
}
