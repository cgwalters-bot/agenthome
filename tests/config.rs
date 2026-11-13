//! Lint for the role agents and prompt tiers, which are edited directly.
//! opencode/CLAUDE.md staying a copy of opencode/AGENTS.md is in rules.rs.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

use agenthome::tiers::resolve_pool;
use agenthome::{parse_jsonc, pretty_json, read_jsonc};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};

const CLAUDE_DIR: &str = "roles/claude/agents";
const OPENCODE_PROFILE: &str = "roles/opencode/eval-tiered.json";
const STANDING_RULES: &str = "opencode/AGENTS.md";
const MAX_PROMPT_BYTES: usize = 4096;
// The pool whose models opencode/opencode.json configures.
const POOL: &str = "praxis";
const LEAD: &str = "role-execute-lead";
const READ_ONLY_TOOLS: &str = "Read, Grep, Glob, Bash";
const EDIT_TOOLS: &str = "Read, Grep, Glob, Bash, Edit, Write";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Access {
    ReadOnly,
    Edit,
    Lead,
}

// Each role's access and prompt tier are pinned, so a widened permission or a
// changed model routing fails here: explore, plan and review cannot edit, and
// only the lead can delegate, so a worker cannot start a chain of its own. The
// prefix keeps the roles from replacing opencode's and Claude Code's built-in
// agents or agentic-job's diff-reviewer.
const ROLES: [(&str, Access, &str); 5] = [
    ("role-explore", Access::ReadOnly, "luna"),
    ("role-plan", Access::ReadOnly, "sol"),
    (LEAD, Access::Lead, "sol"),
    ("role-worker", Access::Edit, "luna"),
    ("role-review", Access::ReadOnly, "sol"),
];

// The keys of opencode v1.18.35's agent schema
// (packages/core/src/v1/config/agent.ts) that the profile uses.
const OPENCODE_AGENT_KEYS: [&str; 5] = ["description", "mode", "model", "prompt", "permission"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[derive(Clone)]
struct Config {
    tiers: Value,
    catalogue: Value,
    runner: Value,
    profile: String,
    /// Claude agent files by file name.
    claude: BTreeMap<String, String>,
}

fn load() -> Config {
    let read = |path: &str| read_jsonc(&root().join(path)).unwrap();
    let claude = fs::read_dir(root().join(CLAUDE_DIR))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            (name, fs::read_to_string(entry.path()).unwrap())
        })
        .collect();
    Config {
        tiers: read("prompt-tiers.json"),
        catalogue: read("opencode/opencode.json"),
        runner: read("opencode/opencode-runner.json"),
        profile: fs::read_to_string(root().join(OPENCODE_PROFILE)).unwrap(),
        claude,
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>> {
    value.as_object().context("expected configuration object")
}

/// Validate every pool, and check that the role assignments resolve to the
/// models opencode.json and the runner profile configure. Returns the
/// resolved tiers of [`POOL`].
fn check_tiers(config: &Config) -> Result<HashMap<String, String>> {
    for pool in object(config.tiers.get("pools").context("missing pools")?)?.keys() {
        resolve_pool(&config.tiers, pool, &config.catalogue)?;
    }
    let resolved = resolve_pool(&config.tiers, POOL, &config.catalogue)?;
    let lookup = |tier: &Value| -> Result<&str> {
        let tier = tier.as_str().context("expected role tier")?;
        resolved
            .get(tier)
            .map(String::as_str)
            .with_context(|| format!("Pool {POOL}: unknown prompt tier {tier:?}"))
    };
    let roles = config
        .tiers
        .get("roles")
        .context("missing role assignments")?;
    for key in ["model", "small_model"] {
        let configured = config.runner.get(key).or(config.catalogue.get(key));
        ensure!(
            configured.and_then(Value::as_str) == Some(lookup(&roles[key])?),
            "roles.{key} disagrees with opencode.json"
        );
    }
    for (name, tier) in object(&roles["agent"])? {
        let configured = [&config.runner, &config.catalogue]
            .into_iter()
            .filter_map(|source| source.get("agent")?.get(name))
            .collect::<Vec<_>>();
        ensure!(!configured.is_empty(), "Unknown opencode role {name}");
        let model = configured.iter().find_map(|agent| agent.get("model"));
        ensure!(
            model.and_then(Value::as_str) == Some(lookup(tier)?),
            "roles.agent.{name} disagrees with opencode.json"
        );
    }
    Ok(resolved)
}

struct ClaudeAgent<'a> {
    description: String,
    access: Access,
    body: &'a str,
}

/// Parse a Claude Code subagent, which must be in the one form written here:
/// name, a JSON-quoted description, the tools of its access level (none for
/// the lead, which inherits all) and `model: inherit`, as the manifest has no
/// verified Claude pool.
fn claude_agent<'a>(name: &str, text: &'a str) -> Result<ClaudeAgent<'a>> {
    let (meta, body) = text
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n\n"))
        .context("expected frontmatter, a blank line and a body")?;
    let body = body.strip_suffix('\n').unwrap_or(body);
    ensure!(!body.trim().is_empty(), "empty body");
    ensure!(
        body.len() <= MAX_PROMPT_BYTES,
        "body larger than {MAX_PROMPT_BYTES} bytes"
    );
    let mut values = BTreeMap::new();
    for line in meta.split('\n') {
        let (key, value) = line
            .split_once(": ")
            .filter(|(key, _)| ["name", "description", "tools", "model"].contains(key))
            .with_context(|| format!("unexpected frontmatter line {line:?}"))?;
        ensure!(
            values.insert(key, value).is_none(),
            "duplicate frontmatter key {key}"
        );
    }
    ensure!(values.get("name") == Some(&name), "name must be {name}");
    ensure!(
        values.get("model") == Some(&"inherit"),
        "model must be inherit"
    );
    let description: String =
        serde_json::from_str(values.get("description").context("missing description")?)
            .context("description must be a JSON string")?;
    ensure!(!description.is_empty(), "empty description");
    let access = match values.get("tools") {
        None => Access::Lead,
        Some(&READ_ONLY_TOOLS) => Access::ReadOnly,
        Some(&EDIT_TOOLS) => Access::Edit,
        Some(tools) => {
            bail!("tools must be {READ_ONLY_TOOLS:?}, {EDIT_TOOLS:?} or absent, not {tools:?}")
        }
    };
    let tools = values
        .get("tools")
        .map_or(String::new(), |tools| format!("tools: {tools}\n"));
    let canonical = format!(
        "---\nname: {name}\ndescription: {}\n{tools}model: inherit\n---\n\n{}\n",
        serde_json::to_string(&description)?,
        body.trim()
    );
    ensure!(
        text == canonical,
        "not in canonical form (frontmatter order, quoting or whitespace)"
    );
    Ok(ClaudeAgent {
        description,
        access,
        body: body.trim(),
    })
}

/// Check the Claude agents and the opencode eval profile: the same roles with
/// the same text, and each role's pinned access and its tier's model in [`POOL`].
fn check_roles(config: &Config, resolved: &HashMap<String, String>) -> Result<()> {
    let names = ROLES.map(|(name, ..)| name);
    let expected: Vec<_> = names.iter().map(|name| format!("{name}.md")).collect();
    let mut found: Vec<_> = config.claude.keys().cloned().collect();
    let mut sorted = expected.clone();
    sorted.sort();
    found.sort();
    ensure!(
        found == sorted,
        "{CLAUDE_DIR} must hold exactly {expected:?}, not {found:?}"
    );

    let profile = parse_jsonc(&config.profile).context(OPENCODE_PROFILE)?;
    ensure!(
        format!("{}\n", pretty_json(&profile)?) == config.profile,
        "{OPENCODE_PROFILE}: not formatted as JSON.stringify(value, null, 2) with a final newline"
    );
    ensure!(
        object(&profile)?.keys().collect::<Vec<_>>() == ["$schema", "default_agent", "agent"],
        "{OPENCODE_PROFILE}: expected only $schema, default_agent and agent; the eval profile leaves models and providers to opencode.json"
    );
    ensure!(
        profile["$schema"] == "https://opencode.ai/config.json" && profile["default_agent"] == LEAD,
        "{OPENCODE_PROFILE}: $schema or default_agent changed"
    );
    let agents = object(&profile["agent"])?;
    ensure!(
        agents.keys().map(String::as_str).eq(names),
        "{OPENCODE_PROFILE}: agents must be {names:?}"
    );

    for (name, access, tier) in ROLES {
        let claude = claude_agent(name, &config.claude[&format!("{name}.md")])
            .with_context(|| format!("{CLAUDE_DIR}/{name}.md"))?;
        ensure!(
            claude.access == access,
            "{CLAUDE_DIR}/{name}.md: access must be {access:?}"
        );
        let agent = &agents[name];
        let context = format!("{OPENCODE_PROFILE}: {name}");
        ensure!(
            object(agent)?
                .keys()
                .map(String::as_str)
                .eq(OPENCODE_AGENT_KEYS),
            "{context}: keys must be {OPENCODE_AGENT_KEYS:?}"
        );
        ensure!(
            agent["description"] == claude.description.as_str() && agent["prompt"] == claude.body,
            "{context}: description and prompt must match the Claude agent"
        );
        let mode = if access == Access::Lead {
            "primary"
        } else {
            "subagent"
        };
        ensure!(agent["mode"] == mode, "{context}: mode must be {mode}");
        let permission = json!({
            "edit": if access == Access::ReadOnly { "deny" } else { "allow" },
            "bash": "allow",
            "task": if access == Access::Lead { "allow" } else { "deny" }
        });
        ensure!(
            agent["permission"] == permission,
            "{context}: permission must be {permission}"
        );
        let model = resolved
            .get(tier)
            .with_context(|| format!("Pool {POOL}: unknown prompt tier {tier:?}"))?;
        ensure!(
            agent["model"] == model.as_str(),
            "{context}: model must be {model}, the {tier} tier's"
        );
    }
    Ok(())
}

fn check(config: &Config) -> Result<()> {
    let resolved = check_tiers(config).context("prompt-tiers.json")?;
    check_roles(config, &resolved)
}

#[test]
fn configuration_is_valid() {
    check(&load()).unwrap();
}

fn edit_profile(config: &mut Config, edit: fn(&mut Value)) {
    let mut profile: Value = serde_json::from_str(&config.profile).unwrap();
    edit(&mut profile);
    config.profile = format!("{}\n", pretty_json(&profile).unwrap());
}

fn edit_claude(config: &mut Config, role: &str, from: &str, to: &str) {
    let text = config.claude.get_mut(&format!("{role}.md")).unwrap();
    assert!(text.contains(from), "{role} contains {from:?}");
    *text = text.replacen(from, to, 1);
}

fn edit_tier(config: &mut Config, tier: &str, key: &str, value: Value) {
    config.tiers["pools"][POOL][tier][key] = value;
}

/// An edit of the valid configuration, and part of the error it must cause.
type Fixture = (&'static str, fn(&mut Config));

#[test]
fn invalid_configuration_fails() {
    let fixtures: [Fixture; 32] = [
        // Prompt tiers.
        ("unknown fallback tier", |c| {
            edit_tier(c, "terra", "fallback", json!("typo"))
        }),
        ("missing terminal model", |c| {
            edit_tier(c, "luna", "model", Value::Null)
        }),
        ("not in the tool catalogue", |c| {
            edit_tier(c, "luna", "model", json!("praxis/unknown"))
        }),
        ("fallback cycle astra -> astra", |c| {
            edit_tier(c, "astra", "fallback", json!("astra"))
        }),
        ("fallback cycle astra -> sol -> luna -> astra", |c| {
            edit_tier(c, "luna", "fallback", json!("astra"))
        }),
        ("explicit model and fallback", |c| {
            edit_tier(c, "sol", "fallback", json!(42))
        }),
        ("explicit model and fallback", |c| {
            edit_tier(c, "sol", "model", json!(""))
        }),
        ("explicit model and fallback", |c| {
            c.tiers["pools"][POOL]["sol"]
                .as_object_mut()
                .unwrap()
                .remove("fallback");
        }),
        ("Unsupported prompt-tier pool", |c| {
            let pools = c.tiers["pools"].as_object_mut().unwrap();
            let pool = pools.remove(POOL).unwrap();
            pools.insert("unverified".into(), pool);
        }),
        ("Unsupported prompt-tier pool", |c| {
            c.tiers["pools"]["empty"] = json!(null)
        }),
        ("unknown prompt tier", |c| {
            c.tiers["roles"]["agent"]["general"] = json!("toString")
        }),
        ("Unknown opencode role", |c| {
            c.tiers["roles"]["agent"]["typo"] = json!("sol")
        }),
        ("roles.small_model disagrees", |c| {
            c.tiers["roles"]["small_model"] = json!("sol")
        }),
        ("roles.agent.explore disagrees", |c| {
            c.tiers["roles"]["agent"]["explore"] = json!("astra")
        }),
        // Claude agents.
        ("must hold exactly", |c| {
            c.claude.remove("role-plan.md");
        }),
        ("must hold exactly", |c| {
            c.claude.insert("role-old.md".into(), "stale".into());
        }),
        ("expected frontmatter", |c| {
            c.claude.insert("role-worker.md".into(), "Body.\n".into());
        }),
        ("unexpected frontmatter line", |c| {
            edit_claude(
                c,
                "role-worker",
                "model: inherit",
                "model: inherit\ncolor: red",
            )
        }),
        ("duplicate frontmatter key", |c| {
            edit_claude(
                c,
                "role-worker",
                "model: inherit",
                "model: inherit\nmodel: inherit",
            )
        }),
        ("model must be inherit", |c| {
            edit_claude(c, "role-worker", "model: inherit", "model: opus")
        }),
        ("tools must be", |c| {
            edit_claude(c, "role-worker", "Edit, Write", "Edit, Write, Task")
        }),
        ("access must be ReadOnly", |c| {
            edit_claude(c, "role-review", READ_ONLY_TOOLS, EDIT_TOOLS)
        }),
        ("not in canonical form", |c| {
            edit_claude(c, "role-worker", "description: \"", "description:  \"")
        }),
        ("body larger than", |c| {
            let text = c.claude.get_mut("role-worker.md").unwrap();
            text.push_str(&"x".repeat(MAX_PROMPT_BYTES));
        }),
        ("must match the Claude agent", |c| {
            let text = c.claude.get_mut("role-worker.md").unwrap();
            text.push_str("More.\n");
        }),
        // The opencode eval profile.
        ("not formatted as JSON.stringify", |c| {
            c.profile = c.profile.replace("\n  ", "\n\t")
        }),
        ("leaves models and providers", |c| {
            edit_profile(c, |p| p["model"] = json!("praxis/gpt-6-luna"))
        }),
        ("permission must be", |c| {
            edit_profile(c, |p| {
                p["agent"]["role-worker"]["permission"]["task"] = json!("allow")
            })
        }),
        ("permission must be", |c| {
            edit_profile(c, |p| {
                p["agent"]["role-plan"]["permission"]["edit"] = json!("allow")
            })
        }),
        ("mode must be primary", |c| {
            edit_profile(c, |p| p["agent"][LEAD]["mode"] = json!("subagent"))
        }),
        ("model must be praxis/gpt-6-luna, the luna tier's", |c| {
            edit_profile(c, |p| {
                p["agent"]["role-worker"]["model"] = json!("praxis/gpt-6-astra")
            })
        }),
        ("model must be praxis/gpt-6.1-sol, the sol tier's", |c| {
            edit_profile(c, |p| {
                p["agent"]["role-review"]["model"] = json!("praxis/gpt-6-luna")
            })
        }),
    ];
    let good = load();
    for (expected, edit) in fixtures {
        let mut config = good.clone();
        edit(&mut config);
        let error = check(&config).expect_err(expected);
        assert!(
            format!("{error:#}").contains(expected),
            "{expected}: {error:#}"
        );
    }
}

#[test]
fn fallbacks_resolve_past_unavailable_tiers() {
    let Config {
        mut tiers,
        catalogue,
        ..
    } = load();
    let resolved = resolve_pool(&tiers, POOL, &catalogue).unwrap();
    assert_eq!(resolved["terra"], resolved["sol"]);
    tiers["pools"][POOL]["astra"]["model"] = Value::Null;
    tiers["pools"][POOL]["sol"]["model"] = Value::Null;
    let resolved = resolve_pool(&tiers, POOL, &catalogue).unwrap();
    assert_eq!(resolved["astra"], resolved["luna"]);
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
        assert_eq!(pretty_json(&value).unwrap(), expected);
    }
}

#[test]
fn json_keys_follow_javascript_order() {
    let value: Value =
        serde_json::from_str(r#"{"b": 1, "10": 2, "a": 3, "9": 4, "01": 5}"#).unwrap();
    let keys: Vec<_> = agenthome::javascript_order(&value)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["9", "10", "b", "a", "01"]);
}

#[test]
fn role_prompts_name_no_model_or_tier() {
    let config = load();
    let mut patterns: Vec<_> = config.catalogue["provider"][POOL]["models"]
        .as_object()
        .unwrap()
        .keys()
        .map(|id| format!("(?i){}", regex::escape(id)))
        .collect();
    patterns.extend(
        [
            r"(?i)\bpraxis\b",
            r"(?i)\bgpt-\d",
            r"(?i)\bclaude-",
            r"(?i)\b(opus|sonnet|haiku|fable|gemini|mistral)\b",
            r"(?i)\b(astra|sol|terra|luna)\b",
        ]
        .map(String::from),
    );
    let patterns: Vec<_> = patterns
        .iter()
        .map(|p| regex::Regex::new(p).unwrap())
        .collect();
    let names_model = |text: &str| {
        patterns
            .iter()
            .find(|p| p.is_match(text))
            .map(|p| p.to_string())
    };
    // The profile's prompts and descriptions equal these, as check_roles shows.
    for (path, text) in &config.claude {
        assert_eq!(names_model(text), None, "{path}");
    }
    for bad in [
        "Use gpt-6-luna.",
        "Ask Opus.",
        "Run on the luna tier.",
        "praxis/x",
        "claude-opus-5-5",
    ] {
        assert!(names_model(bad).is_some(), "{bad}");
    }
}

#[test]
fn standing_rules_carry_no_role_text() {
    let config = load();
    let rules = fs::read_to_string(root().join(STANDING_RULES)).unwrap();
    let rules = rules.split_whitespace().collect::<Vec<_>>().join(" ");
    let paragraph = regex::Regex::new(r"\n\s*\n").unwrap();
    for (name, ..) in ROLES {
        let body = claude_agent(name, &config.claude[&format!("{name}.md")])
            .unwrap()
            .body;
        for text in paragraph.split(body) {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let sentence = text.split(". ").next().unwrap();
            assert!(
                !rules.contains(sentence),
                "{STANDING_RULES} repeats {name}: {sentence}"
            );
        }
    }
}
