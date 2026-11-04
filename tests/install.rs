use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use agenthome::install::{
    Env, Host, Options, Source, Tool, install_archive, install_binary, install_tools,
    meets_version, parse_manifest, run_bounded,
};
use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> String {
    fs::read_to_string(root().join("tools.toml")).unwrap()
}

fn tools() -> Vec<Tool> {
    parse_manifest(&manifest()).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const BINARY_ENTRY: &str = r#"
[[tools]]
name = "fixture"
command = "fixture"
minimum_version = "1.0.0"
version_args = ["--version"]
method = "binary"
url = "https://example.com/fixture"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"#;

const AST_GREP_SOURCE: &str = r#"method = "archive"
url = "https://github.com/ast-grep/ast-grep/releases/download/0.50.0/app-x86_64-unknown-linux-gnu.zip"
sha256 = "0fd3f489639abb8465a914b74b8779171577582318d8a940d00399f59dba81c5"
format = "zip"
member = "ast-grep""#;

#[test]
fn manifest_parses_and_refuses_unpinned_or_unsupported_entries() {
    let tools = tools();
    assert_eq!(
        tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["ast-outline", "ast-grep", "uv"]
    );
    assert!(tools[2].dependency_only);
    let manifest = manifest();
    for (edit, expected) in [
        ((manifest.as_str(), ""), "empty tool manifest"),
        (
            ("\"ast-outline==1.9.0\"", "\"ast-outline\""),
            "unpinned package",
        ),
        (
            ("method = \"uv\"", "method = \"shell\""),
            "unsupported method",
        ),
        (("uv-x86_64-unknown-linux-gnu/uv\"", "../uv\""), "member"),
        (("uv-x86_64-unknown-linux-gnu/uv\"", "/uv\""), "member"),
        (
            ("dependency_only = true", "dependency_only = \"true\""),
            "dependency_only",
        ),
        (
            ("method = \"uv\"", "method = \"uv\"\nmirror = \"x\""),
            "mirror",
        ),
        (
            ("method = \"uv\"", "method = \"uv\"\nmethod = \"uv\""),
            "duplicate key",
        ),
        (
            (
                "name = \"uv\"\ncommand = \"uv\"",
                "name = \"ast-grep\"\ncommand = \"ast-grep\"",
            ),
            "duplicate tool ast-grep",
        ),
        (
            ("command = \"uv\"", "command = \"uvx\""),
            "command must equal name",
        ),
        (
            ("minimum_version = \"0.13.1\"", "minimum_version = \"0.13\""),
            "minimum_version",
        ),
        (
            ("constraints = \"constraints/ast-outline.txt\"\n", ""),
            "constraints",
        ),
        (
            (
                "\"constraints/ast-outline.txt\"",
                "\"/tmp/ast-outline.txt\"",
            ),
            "constraints",
        ),
        (
            ("exclude_newer = \"2026-10-21T00:00:00Z\"\n", ""),
            "exclude_newer",
        ),
        (("\"2026-10-21T00:00:00Z\"", "\"7 days\""), "exclude_newer"),
        (
            (
                "sha256 = \"0fd3f489639abb8465a914b74b8779171577582318d8a940d00399f59dba81c5\"\n",
                "",
            ),
            "sha256",
        ),
        (
            (
                AST_GREP_SOURCE,
                "method = \"npm\"\npackage = \"@ast-grep/cli\"",
            ),
            "unsupported method",
        ),
        (
            ("method = \"uv\"", "method = \"uv\"\nformat = \"zip\""),
            "uv entries take no format",
        ),
    ] {
        let text = manifest.replacen(edit.0, edit.1, 1);
        assert_ne!(text, manifest, "{edit:?} applies");
        let error = format!("{:#}", parse_manifest(&text).unwrap_err());
        assert!(error.contains(expected), "{edit:?}: {error}");
    }
    let error = format!(
        "{:#}",
        parse_manifest(&format!("{manifest}\n[unsupported]\n")).unwrap_err()
    );
    assert!(error.contains("unsupported"), "{error}");
}

#[test]
fn binary_entries_need_an_https_url_and_a_sha256_and_take_no_archive_fields() {
    let tools = parse_manifest(&(manifest() + BINARY_ENTRY)).unwrap();
    assert!(matches!(tools[3].source, Source::Binary { .. }));
    for (from, to) in [
        ("https://", "http://"),
        ("url = \"https://example.com/fixture\"\n", ""),
        ("sha256 = ", "# sha256 = "),
        ("aaaaaaaa\"", "aaaaaaa\""),
        (
            "method = \"binary\"",
            "method = \"binary\"\nformat = \"zip\"",
        ),
        (
            "method = \"binary\"",
            "method = \"binary\"\nmember = \"fixture\"",
        ),
    ] {
        let entry = BINARY_ENTRY.replacen(from, to, 1);
        assert_ne!(entry, BINARY_ENTRY);
        let error = format!("{:#}", parse_manifest(&(manifest() + &entry)).unwrap_err());
        assert!(error.contains("binary"), "{from:?} -> {to:?}: {error}");
    }
}

#[test]
fn uv_constraints_pin_every_resolved_package_exactly() {
    let pin = regex::Regex::new(r"^[a-z0-9-]+==\d+(\.\d+)+$").unwrap();
    for tool in tools() {
        let Source::Uv {
            package,
            constraints,
            ..
        } = &tool.source
        else {
            continue;
        };
        let text = fs::read_to_string(root().join(constraints)).unwrap();
        let lines: Vec<_> = text
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert!(lines.contains(&package.as_str()), "{constraints}");
        for line in lines {
            assert!(pin.is_match(line), "{constraints}: {line}");
        }
    }
}

#[test]
fn minimum_versions_compare_numerically_and_refuse_prereleases() {
    for (output, expected) in [
        ("ast-grep 0.50.0", true),
        ("ast-grep 0.51.0\n", true),
        ("ast-grep 0.100.0", true),
        ("1.0.0", true),
        ("0.9.0", false),
        ("0.50.0-beta", false),
        ("v0.50.0", false),
        ("nonsense", false),
        ("", false),
    ] {
        assert_eq!(meets_version(output, "0.50.0"), expected, "{output:?}");
    }
}

fn var<'a>(env: &'a Env, key: &str) -> Option<&'a str> {
    env.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

const UV_INSTALL: [&str; 9] = [
    "tool",
    "install",
    "--force",
    "--no-build",
    "--constraints",
    "/repo/constraints/ast-outline.txt",
    "--exclude-newer",
    "2026-10-21T00:00:00Z",
    "ast-outline==1.9.0",
];

/// Commands answer `--version` from `versions`; installing sets the
/// minimum version unless the tool is in `failures` or `inert` is set.
#[derive(Default)]
struct Fake {
    versions: HashMap<String, String>,
    calls: Vec<String>,
    lines: Vec<String>,
    installs: Vec<String>,
    failures: HashMap<String, String>,
    inert: bool,
}

impl Fake {
    fn with(versions: &[(&str, &str)]) -> Self {
        Self {
            versions: versions
                .iter()
                .map(|(name, version)| (name.to_string(), version.to_string()))
                .collect(),
            ..Self::default()
        }
    }

    fn install(&mut self, method: &str, tool: &Tool) -> Result<()> {
        self.installs.push(format!("{method} {}", tool.name));
        if let Some(error) = self.failures.get(&tool.name) {
            bail!("{error}");
        }
        if !self.inert {
            self.versions
                .insert(tool.name.clone(), tool.minimum_version.clone());
        }
        Ok(())
    }

    fn take_lines(&mut self) -> Vec<String> {
        self.calls.clear();
        std::mem::take(&mut self.lines)
    }
}

impl Host for Fake {
    fn run(&mut self, program: &str, args: &[String], env: &Env) -> Result<Vec<u8>> {
        self.calls.push(program.to_owned());
        assert_eq!(var(env, "PATH"), Some("/prefix/bin:/usr/bin"));
        assert_eq!(var(env, "UV_TOOL_BIN_DIR"), Some("/prefix/bin"));
        assert_eq!(
            var(env, "UV_TOOL_DIR"),
            Some("/prefix/share/agenthome/uv-tools")
        );
        if args == ["--version"] {
            let Some(version) = self.versions.get(program) else {
                bail!("{program}: not found");
            };
            return Ok(format!("{program} {version}\n").into_bytes());
        }
        assert_eq!((program, args), ("uv", &UV_INSTALL.map(String::from)[..]));
        let tool = tools()
            .into_iter()
            .find(|t| t.name == "ast-outline")
            .unwrap();
        self.install("uv", &tool)?;
        Ok(Vec::new())
    }

    fn install_archive(&mut self, tool: &Tool, _: &Path, _: &Env) -> Result<()> {
        self.install("archive", tool)
    }

    fn install_binary(&mut self, tool: &Tool, _: &Path, _: &Env) -> Result<()> {
        self.install("binary", tool)
    }

    fn log(&mut self, line: &str) {
        self.lines.push(line.to_owned());
    }
}

fn options() -> Options {
    Options {
        bin: PathBuf::from("/prefix/bin"),
        root: PathBuf::from("/repo"),
        path: "/usr/bin".into(),
    }
}

#[test]
fn missing_tools_bootstrap_uv_and_a_second_run_only_checks() {
    let mut fake = Fake::default();
    assert!(install_tools(&tools(), &options(), &mut fake));
    assert_eq!(
        fake.take_lines(),
        [
            "uv: installed",
            "ast-outline: installed",
            "ast-grep: installed"
        ]
    );
    fake.installs.clear();
    assert!(install_tools(&tools(), &options(), &mut fake));
    assert_eq!(fake.lines, ["ast-outline: present", "ast-grep: present"]);
    assert_eq!(fake.calls, ["ast-outline", "ast-grep"]);
    assert!(fake.installs.is_empty());
}

#[test]
fn old_commands_upgrade_and_newer_ones_are_left_alone() {
    let mut fake = Fake::with(&[
        ("ast-outline", "1.8.0"),
        ("uv", "0.13.1"),
        ("ast-grep", "0.51.0"),
    ]);
    assert!(install_tools(&tools(), &options(), &mut fake));
    assert_eq!(
        fake.take_lines(),
        ["uv: present", "ast-outline: installed", "ast-grep: present"]
    );
    assert!(install_tools(&tools(), &options(), &mut fake));
    assert!(!fake.calls.iter().any(|program| program == "uv"));
}

#[test]
fn installation_outcomes() {
    struct Case {
        name: &'static str,
        versions: &'static [(&'static str, &'static str)],
        failures: &'static [(&'static str, &'static str)],
        inert: bool,
        ok: bool,
        lines: &'static [&'static str],
    }
    for case in [
        Case {
            name: "a failed dependency fails the run but independent tools still install",
            versions: &[],
            failures: &[("uv", "download failed")],
            inert: false,
            ok: false,
            lines: &[
                "uv: failed (download failed)",
                "ast-outline: failed (uv dependency unavailable)",
                "ast-grep: installed",
            ],
        },
        Case {
            name: "old ast-grep upgrades through its release archive",
            versions: &[("ast-outline", "1.9.0"), ("ast-grep", "0.49.0")],
            failures: &[],
            inert: false,
            ok: true,
            lines: &["ast-outline: present", "ast-grep: installed"],
        },
        Case {
            name: "an archive failure fails the run",
            versions: &[("ast-outline", "1.9.0")],
            failures: &[("ast-grep", "release checksum mismatch")],
            inert: false,
            ok: false,
            lines: &[
                "ast-outline: present",
                "ast-grep: failed (release checksum mismatch)",
            ],
        },
        Case {
            name: "post-install version verification is mandatory",
            versions: &[("ast-outline", "1.9.0")],
            failures: &[],
            inert: true,
            ok: false,
            lines: &[
                "ast-outline: present",
                "ast-grep: failed (installed command does not meet minimum version)",
            ],
        },
        Case {
            name: "a multi-line error keeps the status to one line",
            versions: &[("ast-outline", "1.9.0")],
            failures: &[("ast-grep", "curl failed\ntranscript")],
            inert: false,
            ok: false,
            lines: &["ast-outline: present", "ast-grep: failed (curl failed)"],
        },
    ] {
        let mut fake = Fake::with(case.versions);
        fake.inert = case.inert;
        fake.failures = case
            .failures
            .iter()
            .map(|(name, error)| (name.to_string(), error.to_string()))
            .collect();
        assert_eq!(
            install_tools(&tools(), &options(), &mut fake),
            case.ok,
            "{}",
            case.name
        );
        assert_eq!(fake.lines, case.lines, "{}", case.name);
    }
}

#[test]
fn binary_entries_install_through_the_binary_method() {
    let mut fake = Fake::with(&[("ast-outline", "1.9.0"), ("ast-grep", "0.50.0")]);
    let tools = parse_manifest(&(manifest() + BINARY_ENTRY)).unwrap();
    assert!(install_tools(&tools, &options(), &mut fake));
    assert_eq!(fake.installs, ["binary fixture"]);
    assert_eq!(
        fake.lines,
        [
            "ast-outline: present",
            "ast-grep: present",
            "fixture: installed"
        ]
    );
}

/// Serves `bytes` for any download and extracts with `extract`, or with the
/// real `unzip` and `tar` when it is `None`.
struct Downloads {
    bytes: Vec<u8>,
    extract: Option<Vec<u8>>,
    downloads: Vec<PathBuf>,
    extracted: Vec<Vec<String>>,
}

impl Downloads {
    fn new(bytes: &[u8], extract: Option<&[u8]>) -> Self {
        Self {
            bytes: bytes.to_vec(),
            extract: extract.map(<[u8]>::to_vec),
            downloads: Vec::new(),
            extracted: Vec::new(),
        }
    }
}

impl Host for Downloads {
    fn run(&mut self, program: &str, args: &[String], env: &Env) -> Result<Vec<u8>> {
        if program == "curl" {
            assert!(args.ends_with(&["https://example.com/fixture".to_owned()]));
            assert!(args.windows(2).any(|w| w == ["--proto", "=https"]));
            let output = args.iter().position(|arg| arg == "--output").unwrap();
            let path = PathBuf::from(&args[output + 1]);
            fs::write(&path, &self.bytes)?;
            self.downloads.push(path);
            return Ok(Vec::new());
        }
        self.extracted.push(
            [program.to_owned()]
                .into_iter()
                .chain(args.to_vec())
                .collect(),
        );
        match &self.extract {
            Some(bytes) => Ok(bytes.clone()),
            None => run_bounded(program, args, env, Duration::from_secs(30), 1 << 20),
        }
    }
}

fn archive_tool(format: &str, sha256: &str, member: &str) -> Tool {
    let entry = format!(
        "[[tools]]\nname = \"ast-grep\"\ncommand = \"ast-grep\"\nminimum_version = \"0.50.0\"\n\
         version_args = [\"--version\"]\nmethod = \"archive\"\nurl = \"https://example.com/fixture\"\n\
         sha256 = \"{sha256}\"\nformat = \"{format}\"\nmember = \"{member}\"\n"
    );
    parse_manifest(&entry).unwrap().remove(0)
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn archives_verify_bytes_before_extracting_the_member() {
    let bytes = b"test archive";
    for (format, extractor) in [("zip", ["unzip", "-p"]), ("tar.gz", ["tar", "-xOf"])] {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let mut host = Downloads::new(bytes, Some(b"test binary"));
        let tool = archive_tool(format, &"0".repeat(64), "ast-grep");
        let error = install_archive(&tool, &bin, &Vec::new(), &mut host).unwrap_err();
        assert!(format!("{error:#}").contains("checksum"), "{error:#}");
        assert!(host.extracted.is_empty());
        let tool = archive_tool(format, &sha256(bytes), "ast-grep");
        install_archive(&tool, &bin, &Vec::new(), &mut host).unwrap();
        let download = host.downloads[1].display().to_string();
        assert_eq!(
            host.extracted,
            [[extractor[0], extractor[1], &download, "ast-grep"].map(String::from)]
        );
        assert_eq!(fs::read(bin.join("ast-grep")).unwrap(), b"test binary");
        assert_eq!(names(&bin), ["ast-grep"]);
        assert!(host.downloads.iter().all(|path| !path.exists()));
    }
}

fn umask() -> u32 {
    let output = Command::new("sh").args(["-c", "umask"]).output().unwrap();
    u32::from_str_radix(String::from_utf8(output.stdout).unwrap().trim(), 8).unwrap()
}

#[test]
fn real_archives_publish_executables_and_keep_the_old_binary_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("ast-grep"), "fixture binary").unwrap();
    for format in ["zip", "tar.gz"] {
        let archive = root.join(format!("fixture.{format}"));
        let status = if format == "zip" {
            Command::new("zip")
                .arg("-qj")
                .arg(&archive)
                .arg(root.join("ast-grep"))
                .status()
        } else {
            Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(root)
                .arg("ast-grep")
                .status()
        };
        assert!(
            status.unwrap().success(),
            "create {format} fixture (needs zip and tar)"
        );
        let bytes = fs::read(&archive).unwrap();
        let bin = root.join("bin");
        let mut host = Downloads::new(&bytes, None);
        let tool = archive_tool(format, &sha256(&bytes), "ast-grep");
        install_archive(&tool, &bin, &Vec::new(), &mut host).unwrap();
        let published = bin.join("ast-grep");
        assert_eq!(fs::read_to_string(&published).unwrap(), "fixture binary");
        assert_eq!(
            fs::metadata(&published).unwrap().permissions().mode() & 0o777,
            0o755 & !umask()
        );
        let missing = archive_tool(format, &sha256(&bytes), "missing");
        assert!(install_archive(&missing, &bin, &Vec::new(), &mut host).is_err());
        assert_eq!(fs::read_to_string(&published).unwrap(), "fixture binary");
        assert_eq!(names(&bin), ["ast-grep"]);
    }
}

#[test]
fn binary_downloads_verify_and_keep_the_old_binary_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    let bytes = b"fixture binary";
    let entry = BINARY_ENTRY.replace(&"a".repeat(64), &sha256(bytes));
    let tool = parse_manifest(&entry).unwrap().remove(0);
    let mut host = Downloads::new(bytes, None);
    install_binary(&tool, &bin, &Vec::new(), &mut host).unwrap();
    let published = bin.join("fixture");
    assert_eq!(fs::read(&published).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&published).unwrap().permissions().mode() & 0o777,
        0o755
    );
    for (served, digest, error) in [
        (&b"substituted"[..], sha256(bytes), "checksum"),
        (&b""[..], sha256(b""), "empty release binary"),
    ] {
        let entry = BINARY_ENTRY.replace(&"a".repeat(64), &digest);
        let tool = parse_manifest(&entry).unwrap().remove(0);
        let mut host = Downloads::new(served, None);
        let failure = install_binary(&tool, &bin, &Vec::new(), &mut host).unwrap_err();
        assert!(format!("{failure:#}").contains(error), "{failure:#}");
        assert!(host.downloads.iter().all(|path| !path.exists()));
    }
    struct Offline;
    impl Host for Offline {
        fn run(&mut self, _: &str, _: &[String], _: &Env) -> Result<Vec<u8>> {
            bail!("download failed")
        }
    }
    let failure = install_binary(&tool, &bin, &Vec::new(), &mut Offline).unwrap_err();
    assert!(format!("{failure:#}").contains("download failed"));
    assert_eq!(fs::read(&published).unwrap(), bytes);
    assert_eq!(names(&bin), ["fixture"]);
}

#[test]
fn commands_are_bounded_in_time_and_output() {
    let sh = |script: &str, timeout: u64, limit: u64| {
        run_bounded(
            "sh",
            &["-c".into(), script.into()],
            &vec![("GREETING".into(), "hello".into())],
            Duration::from_millis(timeout),
            limit,
        )
    };
    assert_eq!(sh("echo $GREETING", 5000, 64).unwrap(), b"hello\n");
    for (script, timeout, limit, expected) in [
        ("sleep 10", 200, 64, "timed out"),
        // A descendant holding the pipe open is held to the same deadline.
        ("sleep 10 & echo started", 200, 64, "timed out"),
        ("yes", 5000, 1024, "wrote more than 1024 bytes"),
        ("yes >&2", 5000, 1024, "wrote more than 1024 bytes"),
        (
            "echo; echo oops >&2; echo more >&2; exit 3",
            5000,
            64,
            "oops",
        ),
    ] {
        let error = format!("{:#}", sh(script, timeout, limit).unwrap_err());
        assert!(error.contains(expected), "{script}: {error}");
        assert!(!error.contains("more\n"), "{script}: {error}");
    }
    let error =
        run_bounded("/nonexistent", &[], &Vec::new(), Duration::from_secs(1), 1).unwrap_err();
    assert!(format!("{error:#}").contains("start /nonexistent"));
}

/// The binary end to end, offline: stub commands on PATH stand in for the
/// tools, and a failing `curl` stub stands in for the network.
#[test]
fn install_tools_cli_reports_one_line_per_tool() {
    let dir = tempfile::tempdir().unwrap();
    let stubs = dir.path().join("stubs");
    fs::create_dir(&stubs).unwrap();
    let stub = |name: &str, script: &str| {
        let path = stubs.join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    };
    stub("ast-outline", "echo ast-outline 1.9.0");
    stub("curl", "echo offline >&2; exit 7");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_agenthome"))
            .arg("install-tools")
            .env("HOME", dir.path())
            .env("PATH", format!("{}:/usr/bin:/bin", stubs.display()))
            .output()
            .unwrap()
    };
    for (version, success, last) in [
        ("0.50.0", true, "ast-grep: present"),
        (
            "0.49.0",
            false,
            "ast-grep: failed (curl failed (exit status: 7): offline)",
        ),
    ] {
        stub("ast-grep", &format!("echo ast-grep {version}"));
        let output = run();
        assert_eq!(output.status.success(), success, "{version}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("ast-outline: present\n{last}\n")
        );
    }
}
