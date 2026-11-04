//! Opt-in installation of the tools in `tools.toml` into a user-local prefix.

use std::{
    collections::HashMap,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Each subprocess, the downloads included, gets at most this long.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
pub const MAX_COMMAND_OUTPUT: u64 = 128 * 1024 * 1024;

/// Extra variables for a subprocess, applied over the inherited environment.
pub type Env = Vec<(String, String)>;

/// The manifest's schema. Unknown tables and keys are refused rather than
/// silently ignored, so a changed manifest cannot mean something unchecked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[serde(default)]
    tools: Vec<RawTool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTool {
    name: String,
    command: String,
    minimum_version: String,
    version_args: Vec<String>,
    method: String,
    #[serde(default)]
    dependency_only: bool,
    package: Option<String>,
    constraints: Option<String>,
    exclude_newer: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
    format: Option<String>,
    member: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    TarGz,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// `uv tool install` of an exact version, its constraints and upload cutoff.
    Uv {
        package: String,
        constraints: String,
        exclude_newer: String,
    },
    /// One member of a SHA-256-pinned release archive.
    Archive {
        url: String,
        sha256: String,
        format: Format,
        member: String,
    },
    /// A SHA-256-pinned single-file release binary.
    Binary { url: String, sha256: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    pub command: String,
    pub minimum_version: String,
    pub version_args: Vec<String>,
    pub dependency_only: bool,
    pub source: Source,
}

fn is_version(text: &str) -> bool {
    let parts: Vec<_> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `YYYY-MM-DDT00:00:00Z`: a whole-day upload cutoff, never a relative span.
fn is_cutoff(text: &str) -> bool {
    text.strip_suffix("T00:00:00Z").is_some_and(|date| {
        date.len() == 10
            && date.bytes().enumerate().all(|(i, b)| match i {
                4 | 7 => b == b'-',
                _ => b.is_ascii_digit(),
            })
    })
}

fn https(url: Option<String>) -> Result<String> {
    url.filter(|url| url.starts_with("https://"))
        .context("url must be https")
}

fn sha256(digest: Option<String>) -> Result<String> {
    digest
        .filter(|digest| is_sha256(digest))
        .context("sha256 must be 64 lowercase hex digits")
}

fn source(raw: RawTool) -> Result<(RawTool, Source)> {
    let mut raw = raw;
    let source = match raw.method.as_str() {
        // A version pin on the top-level package alone lets any new upload of
        // a dependency change what runs on the host: uv entries also need a
        // checked-in constraints file and a fixed upload cutoff.
        "uv" => {
            let package = raw.package.take().context("uv entries need a package")?;
            ensure!(
                package == format!("{}=={}", raw.name, raw.minimum_version),
                "unpinned package {package:?}: expected {}=={}",
                raw.name,
                raw.minimum_version
            );
            let constraints = raw
                .constraints
                .take()
                .context("uv entries need constraints")?;
            ensure!(
                constraints == format!("constraints/{}.txt", raw.name),
                "constraints must be constraints/{}.txt",
                raw.name
            );
            let exclude_newer = raw
                .exclude_newer
                .take()
                .filter(|cutoff| is_cutoff(cutoff))
                .context("exclude_newer must be YYYY-MM-DDT00:00:00Z")?;
            Source::Uv {
                package,
                constraints,
                exclude_newer,
            }
        }
        "archive" => {
            let format = match raw.format.take().as_deref() {
                Some("zip") => Format::Zip,
                Some("tar.gz") => Format::TarGz,
                _ => bail!("archive format must be zip or tar.gz"),
            };
            let member = raw
                .member
                .take()
                .filter(|member| {
                    !member.is_empty()
                        && member.bytes().all(|b| {
                            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'/' | b'-')
                        })
                        && !member.starts_with('/')
                        && !member.split('/').any(|part| part == "..")
                })
                .context("archive member must be a relative path without ..")?;
            Source::Archive {
                url: https(raw.url.take())?,
                sha256: sha256(raw.sha256.take())?,
                format,
                member,
            }
        }
        "binary" => Source::Binary {
            url: https(raw.url.take())?,
            sha256: sha256(raw.sha256.take())?,
        },
        method => bail!("unsupported method {method:?}: expected uv, archive or binary"),
    };
    Ok((raw, source))
}

fn tool(raw: RawTool) -> Result<Tool> {
    ensure!(is_name(&raw.name), "name must match [a-z][a-z0-9-]*");
    ensure!(raw.command == raw.name, "command must equal name");
    ensure!(
        is_version(&raw.minimum_version),
        "minimum_version must be MAJOR.MINOR.PATCH"
    );
    let (raw, source) = source(raw)?;
    // Every field the method consumes was taken above; anything left over
    // belongs to another method.
    for (key, value) in [
        ("package", &raw.package),
        ("constraints", &raw.constraints),
        ("exclude_newer", &raw.exclude_newer),
        ("url", &raw.url),
        ("sha256", &raw.sha256),
        ("format", &raw.format),
        ("member", &raw.member),
    ] {
        ensure!(value.is_none(), "{} entries take no {key}", raw.method);
    }
    Ok(Tool {
        name: raw.name,
        command: raw.command,
        minimum_version: raw.minimum_version,
        version_args: raw.version_args,
        dependency_only: raw.dependency_only,
        source,
    })
}

pub fn parse_manifest(text: &str) -> Result<Vec<Tool>> {
    let manifest: RawManifest = toml::from_str(text).context("parse tool manifest")?;
    ensure!(!manifest.tools.is_empty(), "empty tool manifest");
    let mut tools: Vec<Tool> = Vec::new();
    for (index, raw) in manifest.tools.into_iter().enumerate() {
        let label = format!("tool {} ({:?}, {})", index + 1, raw.name, raw.method);
        let tool = tool(raw).context(label)?;
        ensure!(
            tools.iter().all(|other| other.name != tool.name),
            "duplicate tool {}",
            tool.name
        );
        tools.push(tool);
    }
    Ok(tools)
}

/// Whether the first whitespace-separated `X.Y.Z` in `output` is at least
/// `minimum`. A pre-release suffix such as `0.50.0-beta` does not count.
pub fn meets_version(output: &str, minimum: &str) -> bool {
    let parse = |text: &str| -> Option<[u64; 3]> {
        if !is_version(text) {
            return None;
        }
        let mut parts = text.split('.').map(|part| part.parse().ok());
        Some([parts.next()??, parts.next()??, parts.next()??])
    };
    let Some(actual) = output.split_whitespace().find_map(parse) else {
        return false;
    };
    parse(minimum).is_some_and(|required| actual >= required)
}

fn first_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
}

/// Run a command without a shell, with a time limit and bounded output, and
/// return its standard output. A failure names the command and the first line
/// of its standard error.
pub fn run_bounded(
    program: &str,
    args: &[String],
    env: &Env,
    timeout: Duration,
    limit: u64,
) -> Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {program}"))?;
    let overflow = Arc::new(AtomicBool::new(false));
    let capture = |pipe: Option<Box<dyn Read + Send>>| {
        let overflow = Arc::clone(&overflow);
        thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            if let Some(pipe) = pipe {
                pipe.take(limit + 1).read_to_end(&mut bytes)?;
            }
            if bytes.len() as u64 > limit {
                overflow.store(true, Ordering::Relaxed);
            }
            Ok(bytes)
        })
    };
    let stdout = capture(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = capture(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + timeout;
    // Wait for the pipes as well as the process: a descendant that keeps
    // them open is held to the same deadline, and its readers are abandoned.
    let mut status = None;
    loop {
        if status.is_none() {
            status = child.try_wait().context("wait for command")?;
        }
        if status.is_some() && stdout.is_finished() && stderr.is_finished() {
            break;
        }
        let overflowed = overflow.load(Ordering::Relaxed);
        if overflowed || Instant::now() >= deadline {
            if status.is_none() {
                child.kill().ok();
                child.wait().ok();
            }
            if overflowed {
                bail!("{program} wrote more than {limit} bytes");
            }
            bail!("{program} timed out after {}s", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(20));
    }
    let status = status.expect("loop exits with a status");
    let stdout = stdout
        .join()
        .expect("stdout reader")
        .context("read output")?;
    let stderr = stderr
        .join()
        .expect("stderr reader")
        .context("read output")?;
    ensure!(
        !overflow.load(Ordering::Relaxed),
        "{program} wrote more than {limit} bytes"
    );
    ensure!(
        status.success(),
        "{program} failed ({status}): {}",
        first_line(&String::from_utf8_lossy(&stderr))
    );
    Ok(stdout)
}

/// What the installer does to the host, replaceable in tests.
pub trait Host {
    fn run(&mut self, program: &str, args: &[String], env: &Env) -> Result<Vec<u8>> {
        run_bounded(program, args, env, COMMAND_TIMEOUT, MAX_COMMAND_OUTPUT)
    }

    fn install_archive(&mut self, tool: &Tool, bin: &Path, env: &Env) -> Result<()> {
        install_archive(tool, bin, env, self)
    }

    fn install_binary(&mut self, tool: &Tool, bin: &Path, env: &Env) -> Result<()> {
        install_binary(tool, bin, env, self)
    }

    fn log(&mut self, line: &str) {
        println!("{line}");
    }
}

/// The real host: subprocesses, downloads and standard output.
pub struct System;

impl Host for System {}

fn pinned(tool: &Tool) -> Result<(&str, &str)> {
    match &tool.source {
        Source::Archive { url, sha256, .. } | Source::Binary { url, sha256 } => Ok((url, sha256)),
        Source::Uv { .. } => bail!("{} is not a release download", tool.name),
    }
}

/// Download a release artifact and stop a substituted or corrupted one
/// before it is used.
fn download<H: Host + ?Sized>(tool: &Tool, path: &Path, env: &Env, host: &mut H) -> Result<()> {
    let (url, expected) = pinned(tool)?;
    let args = [
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-time",
        "120",
        "--max-filesize",
        "134217728",
        "--output",
    ]
    .map(String::from)
    .into_iter()
    .chain([path.display().to_string(), url.to_owned()])
    .collect::<Vec<_>>();
    host.run("curl", &args, env)?;
    let bytes = fs::read(path).context("read download")?;
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    ensure!(digest == expected, "release checksum mismatch");
    Ok(())
}

fn staging(bin: &Path) -> Result<tempfile::TempDir> {
    fs::create_dir_all(bin).with_context(|| format!("create {}", bin.display()))?;
    tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(bin)
        .context("create staging directory")
}

/// Extract only the named member, never archive paths into the destination,
/// and replace the command in one rename.
pub fn install_archive<H: Host + ?Sized>(
    tool: &Tool,
    bin: &Path,
    env: &Env,
    host: &mut H,
) -> Result<()> {
    let Source::Archive { format, member, .. } = &tool.source else {
        bail!("{} is not an archive entry", tool.name);
    };
    let temporary = tempfile::Builder::new()
        .prefix("agenthome-tools-")
        .tempdir()
        .context("create download directory")?;
    let archive = temporary.path().join("archive");
    download(tool, &archive, env, host)?;
    let archive = archive.display().to_string();
    let contents = match format {
        Format::Zip => host.run("unzip", &["-p".into(), archive, member.clone()], env)?,
        Format::TarGz => host.run("tar", &["-xOf".into(), archive, member.clone()], env)?,
    };
    ensure!(!contents.is_empty(), "empty release binary");
    let staging = staging(bin)?;
    let target = staging.path().join(&tool.command);
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&target)
            .context("create staged binary")?;
        std::io::Write::write_all(&mut file, &contents).context("write staged binary")?;
    }
    fs::rename(&target, bin.join(&tool.command)).context("publish binary")?;
    Ok(())
}

pub fn install_binary<H: Host + ?Sized>(
    tool: &Tool,
    bin: &Path,
    env: &Env,
    host: &mut H,
) -> Result<()> {
    let staging = staging(bin)?;
    let target = staging.path().join(&tool.command);
    download(tool, &target, env, host)?;
    ensure!(
        fs::metadata(&target).context("stat download")?.len() > 0,
        "empty release binary"
    );
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).context("make executable")?;
    fs::rename(&target, bin.join(&tool.command)).context("publish binary")?;
    Ok(())
}

pub struct Options {
    /// Where commands are installed; prepended to `PATH`.
    pub bin: PathBuf,
    /// The checkout holding the constraints files.
    pub root: PathBuf,
    /// The caller's `PATH`.
    pub path: String,
}

struct Installer<'a, H: Host> {
    tools: &'a [Tool],
    options: &'a Options,
    env: Env,
    host: &'a mut H,
    results: HashMap<String, bool>,
}

impl<H: Host> Installer<'_, H> {
    fn present(&mut self, tool: &Tool) -> bool {
        self.host
            .run(&tool.command, &tool.version_args, &self.env)
            .is_ok_and(|output| {
                meets_version(&String::from_utf8_lossy(&output), &tool.minimum_version)
            })
    }

    fn install(&mut self, tool: &Tool) -> Result<()> {
        let bin = &self.options.bin;
        match &tool.source {
            Source::Uv {
                package,
                constraints,
                exclude_newer,
            } => {
                let uv = self.tools.iter().find(|tool| tool.name == "uv");
                ensure!(
                    uv.is_some_and(|uv| self.ensure(uv)),
                    "uv dependency unavailable"
                );
                // --no-build: no sdist build script runs on the host.
                let args = ["tool", "install", "--force", "--no-build", "--constraints"]
                    .map(String::from)
                    .into_iter()
                    .chain([
                        self.options.root.join(constraints).display().to_string(),
                        "--exclude-newer".into(),
                        exclude_newer.clone(),
                        package.clone(),
                    ])
                    .collect::<Vec<_>>();
                self.host.run("uv", &args, &self.env)?;
            }
            Source::Archive { .. } => self.host.install_archive(tool, bin, &self.env)?,
            Source::Binary { .. } => self.host.install_binary(tool, bin, &self.env)?,
        }
        ensure!(
            self.present(tool),
            "installed command does not meet minimum version"
        );
        Ok(())
    }

    fn ensure(&mut self, tool: &Tool) -> bool {
        if let Some(&result) = self.results.get(&tool.name) {
            return result;
        }
        let status = if self.present(tool) {
            Ok("present")
        } else {
            self.install(tool).map(|()| "installed")
        };
        let line = match &status {
            Ok(status) => format!("{}: {status}", tool.name),
            // Keep subprocess transcripts out of the one-line summary.
            Err(error) => format!(
                "{}: failed ({})",
                tool.name,
                first_line(&format!("{error:#}"))
            ),
        };
        self.host.log(&line);
        self.results.insert(tool.name.clone(), status.is_ok());
        status.is_ok()
    }
}

/// Install every tool that is not only a dependency, one status line each.
/// A failure does not stop independent tools; the result is false if any
/// attempted tool failed. There is no retry loop.
pub fn install_tools<H: Host>(tools: &[Tool], options: &Options, host: &mut H) -> bool {
    let bin = options.bin.display().to_string();
    let share = options
        .bin
        .parent()
        .unwrap_or(&options.bin)
        .join("share/agenthome/uv-tools");
    let mut installer = Installer {
        tools,
        options,
        env: vec![
            ("PATH".into(), format!("{bin}:{}", options.path)),
            ("UV_TOOL_BIN_DIR".into(), bin),
            ("UV_TOOL_DIR".into(), share.display().to_string()),
        ],
        host,
        results: HashMap::new(),
    };
    for tool in tools.iter().filter(|tool| !tool.dependency_only) {
        installer.ensure(tool);
    }
    installer.results.values().all(|&ok| ok)
}
