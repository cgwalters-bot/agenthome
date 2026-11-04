use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};

use crate::{read_jsonc, tiers::resolve_prompt_tier};

pub const ROLES: [&str; 5] = ["explore", "plan", "execute-lead", "worker", "review"];
pub const TEMPLATE_DIR: &str = "roles/templates";
pub const CLAUDE_DIR: &str = "roles/claude/agents";
pub const OPENCODE_PROFILE: &str = "roles/opencode/eval-tiered.json";
pub const STANDING_RULES: &str = "opencode/AGENTS.md";
pub const CLAUDE_RULES: &str = "opencode/CLAUDE.md";
pub const MAX_TEMPLATE_BYTES: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    Edit,
    Lead,
}

#[derive(Debug)]
pub struct Role {
    pub name: String,
    pub description: String,
    pub tier: String,
    pub access: Access,
    pub body: String,
}

pub fn parse_template(name: &str, text: &str) -> Result<Role> {
    parse_template_inner(name, text).with_context(|| format!("{TEMPLATE_DIR}/{name}.md"))
}

fn parse_template_inner(name: &str, text: &str) -> Result<Role> {
    ensure!(
        text.len() <= MAX_TEMPLATE_BYTES,
        "larger than {MAX_TEMPLATE_BYTES} bytes"
    );
    let (meta, body) = text
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n\n"))
        .filter(|(_, body)| !body.is_empty())
        .context("expected frontmatter, a blank line and a body")?;
    let mut values = BTreeMap::new();
    for line in meta.split('\n') {
        let (key, value) = line
            .split_once(": ")
            .with_context(|| format!("unexpected frontmatter line {line:?}"))?;
        ensure!(
            ["description", "tier", "access"].contains(&key) && !values.contains_key(key),
            "unexpected frontmatter line {line:?}"
        );
        values.insert(key, value.trim());
    }
    for key in ["description", "tier", "access"] {
        ensure!(
            values.get(key).is_some_and(|value| !value.is_empty()),
            "missing {key}"
        );
    }
    let access = match values["access"] {
        "read-only" => Access::ReadOnly,
        "edit" => Access::Edit,
        "lead" => Access::Lead,
        _ => bail!("access must be one of read-only, edit, lead"),
    };
    Ok(Role {
        name: format!("role-{name}"),
        description: values["description"].to_owned(),
        tier: values["tier"].to_owned(),
        access,
        body: body.trim().to_owned(),
    })
}

fn claude_agent(role: &Role) -> Result<String> {
    let tools = match role.access {
        Access::ReadOnly => "tools: Read, Grep, Glob, Bash\n",
        Access::Edit => "tools: Read, Grep, Glob, Bash, Edit, Write\n",
        Access::Lead => "",
    };
    Ok(format!(
        "---\nname: {}\ndescription: {}\n{tools}model: inherit\n---\n\n{}\n",
        role.name,
        serde_json::to_string(&role.description)?,
        role.body
    ))
}

fn opencode_agent(role: &Role, model: &str) -> Value {
    json!({
        "description": role.description,
        "mode": if role.access == Access::Lead { "primary" } else { "subagent" },
        "model": model,
        "prompt": role.body,
        "permission": {
            "edit": if role.access == Access::ReadOnly { "deny" } else { "allow" },
            "bash": "allow",
            "task": if role.access == Access::Lead { "allow" } else { "deny" }
        }
    })
}

fn read(root: &Path, path: &str) -> Result<String> {
    fs::read_to_string(root.join(path)).with_context(|| format!("read {path}"))
}

pub fn render_roles(root: &Path) -> Result<BTreeMap<String, String>> {
    let manifest = read_jsonc(&root.join("prompt-tiers.json"))?;
    let catalogue = read_jsonc(&root.join("opencode/opencode.json"))?;
    let mut roles = Vec::new();
    for name in ROLES {
        roles.push(parse_template(
            name,
            &read(root, &format!("{TEMPLATE_DIR}/{name}.md"))?,
        )?);
    }
    for entry in fs::read_dir(root.join(TEMPLATE_DIR)).context("list role templates")? {
        let name = entry?.file_name();
        let name = name.to_str().context("non-UTF-8 template name")?;
        ensure!(
            ROLES.contains(&name.strip_suffix(".md").unwrap_or(name)),
            "{TEMPLATE_DIR}: unknown templates {name}"
        );
    }
    let mut files = BTreeMap::new();
    let mut agents = Map::new();
    for role in roles {
        files.insert(
            format!("{CLAUDE_DIR}/{}.md", role.name),
            claude_agent(&role)?,
        );
        let model = resolve_prompt_tier(&manifest, "praxis", &role.tier, &catalogue)?;
        agents.insert(role.name.clone(), opencode_agent(&role, &model));
    }
    let profile = json!({
        "$schema": "https://opencode.ai/config.json",
        "default_agent": "role-execute-lead",
        "agent": agents
    });
    files.insert(
        OPENCODE_PROFILE.to_owned(),
        format!("{}\n", crate::pretty_json(&profile)?),
    );
    files.insert(CLAUDE_RULES.to_owned(), read(root, STANDING_RULES)?);
    Ok(files)
}

pub fn drift(root: &Path, files: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for (path, content) in files {
        let current = match fs::read_to_string(root.join(path)) {
            Ok(current) => Some(current),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error).with_context(|| format!("read {path}")),
        };
        if current.as_ref() != Some(content) {
            changed.push(path.clone());
        }
    }
    match fs::read_dir(root.join(CLAUDE_DIR)) {
        Ok(entries) => {
            for entry in entries {
                let name = entry?.file_name();
                let path = format!(
                    "{CLAUDE_DIR}/{}",
                    name.to_str().context("non-UTF-8 agent name")?
                );
                if !files.contains_key(&path) {
                    changed.push(path);
                }
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("list rendered Claude agents"),
    }
    changed.sort();
    Ok(changed)
}

pub fn write_roles(root: &Path, files: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let changed = drift(root, files)?;
    for path in &changed {
        let target = root.join(path);
        if let Some(content) = files.get(path) {
            fs::create_dir_all(target.parent().context("output has no parent")?)
                .with_context(|| format!("create parent of {path}"))?;
            fs::write(&target, content).with_context(|| format!("write {path}"))?;
        } else {
            fs::remove_file(&target).with_context(|| format!("remove stale role {path}"))?;
        }
    }
    Ok(changed)
}
