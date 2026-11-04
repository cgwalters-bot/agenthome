use std::{fs, path::Path, process::Command};

use agenthome::{
    parse_jsonc, read_jsonc,
    render::{
        self, CLAUDE_DIR, CLAUDE_RULES, OPENCODE_PROFILE, ROLES, STANDING_RULES, TEMPLATE_DIR,
    },
    tiers::{render_opencode, resolve_prompt_tier},
};
use serde_json::{Value, json};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn inputs() -> (Value, Value, Value) {
    (
        read_jsonc(&root().join("prompt-tiers.json")).unwrap(),
        read_jsonc(&root().join("opencode/opencode.json")).unwrap(),
        read_jsonc(&root().join("opencode/opencode-runner.json")).unwrap(),
    )
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in ["opencode", "roles"] {
        copy_tree(&root().join(name), &dir.path().join(name));
    }
    fs::copy(
        root().join("prompt-tiers.json"),
        dir.path().join("prompt-tiers.json"),
    )
    .unwrap();
    dir
}

#[test]
fn comments_do_not_strip_urls() {
    assert_eq!(
        parse_jsonc("{\n // comment\n \"url\": \"http://x/y\"\n}").unwrap(),
        json!({"url": "http://x/y"})
    );
}

#[test]
fn numbers_use_javascript_spelling() {
    for (input, expected) in [
        ("2.0", "2"),
        ("-0.0", "0"),
        ("0.1", "0.1"),
        ("1e-7", "1e-7"),
        ("1e-6", "0.000001"),
        ("1e20", "100000000000000000000"),
        ("1e21", "1e+21"),
    ] {
        let value = serde_json::from_str(input).unwrap();
        assert_eq!(agenthome::pretty_json(&value).unwrap(), expected);
    }
}

#[test]
fn pool_and_fallback_resolution() {
    let (mut manifest, base, _) = inputs();
    for (tier, expected) in [
        (
            "astra",
            manifest["pools"]["praxis"]["astra"]["model"].clone(),
        ),
        ("sol", base["model"].clone()),
        ("terra", base["model"].clone()),
        ("luna", base["small_model"].clone()),
    ] {
        assert_eq!(
            resolve_prompt_tier(&manifest, "praxis", tier, &base).unwrap(),
            expected.as_str().unwrap()
        );
    }
    manifest["pools"]["praxis"]["astra"]["model"] = Value::Null;
    manifest["pools"]["praxis"]["sol"]["model"] = Value::Null;
    assert_eq!(
        resolve_prompt_tier(&manifest, "praxis", "astra", &base).unwrap(),
        base["small_model"].as_str().unwrap()
    );
}

#[test]
fn malformed_paths_fail_including_unused_edges() {
    let (manifest, base, _) = inputs();
    for (pool, tier, expected) in [
        ("unverified", "sol", "Unsupported prompt-tier pool"),
        ("toString", "sol", "Unsupported prompt-tier pool"),
        ("praxis", "typo", "unknown prompt tier"),
        ("praxis", "toString", "unknown prompt tier"),
    ] {
        let error = resolve_prompt_tier(&manifest, pool, tier, &base).unwrap_err();
        assert!(format!("{error:#}").contains(expected));
    }
    for (tier, key, value, expected) in [
        ("terra", "fallback", json!("typo"), "unknown fallback tier"),
        ("luna", "model", Value::Null, "missing terminal model"),
        (
            "luna",
            "model",
            json!("praxis/unknown"),
            "not in the tool catalogue",
        ),
        (
            "astra",
            "fallback",
            json!("astra"),
            "fallback cycle astra -> astra",
        ),
        (
            "luna",
            "fallback",
            json!("astra"),
            "fallback cycle astra -> sol -> luna -> astra",
        ),
        (
            "terra",
            "fallback",
            json!("terra"),
            "fallback cycle terra -> terra",
        ),
        ("sol", "fallback", json!(42), "explicit model and fallback"),
        ("sol", "model", json!(""), "explicit model and fallback"),
    ] {
        let mut edited = manifest.clone();
        edited["pools"]["praxis"][tier][key] = value;
        let error = resolve_prompt_tier(&edited, "praxis", "sol", &base).unwrap_err();
        assert!(format!("{error:#}").contains(expected), "{error:#}");
    }
    let mut edited = manifest.clone();
    edited["pools"]["praxis"]["sol"]
        .as_object_mut()
        .unwrap()
        .remove("fallback");
    assert!(resolve_prompt_tier(&edited, "praxis", "sol", &base).is_err());
    let mut missing = base.clone();
    let model = base["small_model"]
        .as_str()
        .unwrap()
        .split_once('/')
        .unwrap()
        .1;
    missing["provider"]["praxis"]["models"]
        .as_object_mut()
        .unwrap()
        .remove(model);
    assert!(resolve_prompt_tier(&manifest, "praxis", "luna", &missing).is_err());
}

#[test]
fn effective_configuration_is_preserved() {
    let (manifest, base, runner) = inputs();
    let mut expected = base.as_object().unwrap().clone();
    expected.extend(runner.as_object().unwrap().clone());
    let mut agents = base["agent"].as_object().unwrap().clone();
    for (name, overrides) in runner["agent"].as_object().unwrap() {
        let agent = agents.entry(name.clone()).or_insert_with(|| json!({}));
        agent
            .as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
    }
    expected.insert("agent".into(), Value::Object(agents));
    assert_eq!(
        render_opencode(&manifest, "praxis", &base, &runner).unwrap(),
        Value::Object(expected)
    );
    let mut edited = manifest.clone();
    edited["roles"]["agent"]["general"] = json!("terra");
    assert_eq!(
        render_opencode(&edited, "praxis", &base, &runner).unwrap()["agent"]["general"]["model"],
        base["model"]
    );
    edited["roles"]["agent"]["general"] = json!("unknown");
    assert!(render_opencode(&edited, "praxis", &base, &runner).is_err());
    edited["roles"]["agent"] = json!({"typo": "sol"});
    assert!(render_opencode(&edited, "praxis", &base, &runner).is_err());
}

#[test]
fn rendered_roles_match_committed_bytes() {
    let files = render::render_roles(root()).unwrap();
    assert_eq!(files.len(), 7);
    for (path, bytes) in &files {
        assert_eq!(
            bytes.as_bytes(),
            fs::read(root().join(path)).unwrap(),
            "{path}"
        );
    }
    assert!(render::drift(root(), &files).unwrap().is_empty());
    assert_eq!(
        files[CLAUDE_RULES],
        fs::read_to_string(root().join(STANDING_RULES)).unwrap()
    );
    let profile: Value = serde_json::from_str(&files[OPENCODE_PROFILE]).unwrap();
    for name in ROLES {
        let template =
            fs::read_to_string(root().join(format!("{TEMPLATE_DIR}/{name}.md"))).unwrap();
        let role = render::parse_template(name, &template).unwrap();
        let claude = &files[&format!("{CLAUDE_DIR}/role-{name}.md")];
        assert_eq!(claude.split_once("\n---\n\n").unwrap().1.trim(), role.body);
        let agent = &profile["agent"][&role.name];
        assert_eq!(agent["prompt"], role.body);
        assert_eq!(agent["description"], role.description);
        assert_eq!(
            agent["permission"]["task"],
            if role.access == render::Access::Lead {
                "allow"
            } else {
                "deny"
            }
        );
        assert_eq!(
            agent["permission"]["edit"],
            if role.access == render::Access::ReadOnly {
                "deny"
            } else {
                "allow"
            }
        );
    }
}

#[test]
fn drift_is_repaired_without_rewriting_current_files() {
    for kind in ["edited", "missing", "stale", "template", "rules", "tier"] {
        let dir = scratch();
        let root = dir.path();
        let expected = match kind {
            "edited" => {
                fs::write(root.join(OPENCODE_PROFILE), "{}\n").unwrap();
                OPENCODE_PROFILE.to_owned()
            }
            "missing" => {
                let path = format!("{CLAUDE_DIR}/role-execute-lead.md");
                fs::remove_file(root.join(&path)).unwrap();
                path
            }
            "stale" => {
                let path = format!("{CLAUDE_DIR}/role-old.md");
                fs::write(root.join(&path), "stale").unwrap();
                path
            }
            "template" => {
                let path = root.join(format!("{TEMPLATE_DIR}/worker.md"));
                let text = fs::read_to_string(&path).unwrap();
                fs::write(path, text + "More.\n").unwrap();
                format!("{CLAUDE_DIR}/role-worker.md")
            }
            "rules" => {
                fs::write(root.join(STANDING_RULES), "Changed.\n").unwrap();
                CLAUDE_RULES.to_owned()
            }
            "tier" => {
                let path = root.join("prompt-tiers.json");
                let mut manifest = read_jsonc(&path).unwrap();
                manifest["pools"]["praxis"]["luna"]["model"] = json!("praxis/gpt-6-astra");
                fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
                OPENCODE_PROFILE.to_owned()
            }
            _ => unreachable!(),
        };
        let files = render::render_roles(root).unwrap();
        assert!(
            render::drift(root, &files).unwrap().contains(&expected),
            "{kind}"
        );
        render::write_roles(root, &files).unwrap();
        assert!(render::drift(root, &files).unwrap().is_empty());
        let before = fs::metadata(root.join(OPENCODE_PROFILE))
            .unwrap()
            .modified()
            .unwrap();
        assert!(render::write_roles(root, &files).unwrap().is_empty());
        assert_eq!(
            before,
            fs::metadata(root.join(OPENCODE_PROFILE))
                .unwrap()
                .modified()
                .unwrap()
        );
    }
}

#[test]
fn invalid_templates_are_rejected() {
    let good = fs::read_to_string(root().join(format!("{TEMPLATE_DIR}/worker.md"))).unwrap();
    for (text, expected) in [
        ("Body.\n".to_owned(), "expected frontmatter"),
        (
            good.replace("access: edit", "model: edit"),
            "unexpected frontmatter",
        ),
        (
            good.replace("access: edit", "access: edit\naccess: lead"),
            "unexpected frontmatter",
        ),
        (good.replace("tier: luna\n", ""), "missing tier"),
        (
            good.replace("access: edit", "access: root"),
            "access must be one of",
        ),
        (
            good.clone() + &"x".repeat(render::MAX_TEMPLATE_BYTES),
            "larger than",
        ),
    ] {
        let error = render::parse_template("worker", &text).unwrap_err();
        assert!(format!("{error:#}").contains(expected), "{error:#}");
    }
    let dir = scratch();
    let path = dir.path().join(format!("{TEMPLATE_DIR}/worker.md"));
    fs::write(&path, good.replace("tier: luna", "tier: praxis/gpt-6-luna")).unwrap();
    assert!(render::render_roles(dir.path()).is_err());
    fs::write(path, &good).unwrap();
    fs::write(dir.path().join(format!("{TEMPLATE_DIR}/extra.md")), good).unwrap();
    assert!(render::render_roles(dir.path()).is_err());
}

#[test]
fn rust_cli_is_deterministic_and_has_no_partial_output_on_error() {
    let binary = env!("CARGO_BIN_EXE_agenthome");
    let run = |args: &[&str]| Command::new(binary).args(args).output().unwrap();
    let check = run(&["render-roles", "--check"]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(check.stdout.is_empty() && check.stderr.is_empty());
    let first = run(&["render-prompt-tiers", "praxis"]);
    assert!(first.status.success());
    assert!(first.stderr.is_empty());
    assert_eq!(first.stdout, run(&["render-prompt-tiers", "praxis"]).stdout);
    for args in [
        vec![],
        vec!["render-prompt-tiers"],
        vec!["render-prompt-tiers", "unknown"],
        vec!["render-prompt-tiers", "praxis", "extra"],
        vec!["render-roles", "unknown"],
    ] {
        let result = run(&args);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
}

// Temporary migration proof: retain the JavaScript reference until all of its
// scripts and suites have been ported. The committed role files above are also
// independently compared byte-for-byte, without needing Node.
#[test]
fn prompt_render_matches_javascript_bytes() {
    let legacy = Command::new("node")
        .arg(root().join("scripts/render-prompt-tiers.mjs"))
        .arg("praxis")
        .output()
        .expect("Node is required until the JavaScript migration is complete");
    assert!(
        legacy.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let (manifest, base, runner) = inputs();
    let rendered = render_opencode(&manifest, "praxis", &base, &runner).unwrap();
    assert_eq!(
        format!("{}\n", agenthome::pretty_json(&rendered).unwrap()),
        String::from_utf8(legacy.stdout).unwrap()
    );
}
