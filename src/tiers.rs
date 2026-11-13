//! Shared offline prompt-tier resolution and opt-in evaluation bundles.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

const ROLE_TIERS: [(&str, &str); 5] = [
    ("role-explore", "luna"),
    ("role-plan", "sol"),
    ("role-execute-lead", "sol"),
    ("role-worker", "luna"),
    ("role-review", "sol"),
];

fn nullable_string<'a>(entry: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match entry.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        _ => bail!("expected explicit model and fallback (nonempty string or null)"),
    }
}

/// Validate every fallback edge, including edges behind available models.
pub fn resolve_pool(
    tiers: &Value,
    pool: &str,
    catalogue: &Value,
) -> Result<HashMap<String, String>> {
    let entries = tiers
        .get("pools")
        .and_then(|p| p.get(pool))
        .and_then(Value::as_object)
        .with_context(|| format!("Unsupported prompt-tier pool {pool:?}"))?;
    ensure!(!entries.is_empty(), "Prompt-tier pool {pool} is empty");
    for entry in entries.values() {
        nullable_string(entry, "model")?;
        if let Some(fallback) = nullable_string(entry, "fallback")? {
            ensure!(
                entries.contains_key(fallback),
                "unknown fallback tier {fallback:?}"
            );
        }
    }
    let mut resolved = HashMap::new();
    for start in entries.keys() {
        let mut seen = HashSet::new();
        let mut path = Vec::new();
        let mut tier = Some(start.as_str());
        let mut model = None;
        while let Some(current) = tier {
            path.push(current);
            ensure!(
                seen.insert(current),
                "Pool {pool}: fallback cycle {}",
                path.join(" -> ")
            );
            let entry = &entries[current];
            if let Some(candidate) = nullable_string(entry, "model")? {
                let configured = candidate.split_once('/').and_then(|(provider, id)| {
                    catalogue
                        .get("provider")?
                        .get(provider)?
                        .get("models")?
                        .get(id)
                });
                ensure!(
                    configured.is_some(),
                    "Pool {pool}, tier {current}: model {candidate} is not in the tool catalogue"
                );
                model.get_or_insert(candidate);
            }
            tier = nullable_string(entry, "fallback")?;
        }
        resolved.insert(
            start.clone(),
            model
                .context("missing terminal model on fallback path")?
                .to_owned(),
        );
    }
    Ok(resolved)
}

/// A complete pair of configuration files, not installation or execution proof.
/// Never writes configuration or changes the ordinary runner defaults.
pub fn eval_bundle(
    tiers: &Value,
    catalogue: &Value,
    runner: &Value,
    tiered: &Value,
    pool: &str,
    arm: &str,
) -> Result<Value> {
    let resolved = resolve_pool(tiers, pool, catalogue)?;
    ensure!(
        crate::eval::ARMS.contains(&arm),
        "unknown evaluation arm {arm:?}"
    );
    let mut base = catalogue.clone();
    let mut profile = runner.clone();
    if arm == "tiered" {
        let roles = tiers.get("roles").context("missing role assignments")?;
        let lookup = |tier: &Value| -> Result<Value> {
            let tier = tier.as_str().context("expected role tier")?;
            Ok(json!(resolved.get(tier).context("unknown role tier")?))
        };
        for key in ["model", "small_model"] {
            base[key] = lookup(&roles[key])?;
        }
        for (name, tier) in roles["agent"].as_object().context("missing agent tiers")? {
            let model = lookup(tier)?;
            for config in [&mut base, &mut profile] {
                if let Some(agent) = config
                    .get_mut("agent")
                    .and_then(|agents| agents.get_mut(name))
                {
                    agent["model"] = model.clone();
                }
            }
        }
        // The skill-defined reviewer is separate from the optional role-review.
        profile["agent"]["diff-reviewer"] =
            json!({"model": resolved.get("sol").context("missing review tier")?});
        profile["default_agent"] = tiered["default_agent"].clone();
        let agents = tiered["agent"]
            .as_object()
            .context("missing tiered agents")?;
        ensure!(
            agents.len() == ROLE_TIERS.len(),
            "expected five tiered roles"
        );
        for (name, tier) in ROLE_TIERS {
            let mut agent = agents.get(name).context("missing tiered role")?.clone();
            agent["model"] = json!(resolved.get(tier).context("missing role tier")?);
            profile["agent"][name] = agent;
        }
    } else {
        let tier = if arm == "single-strong" {
            "sol"
        } else {
            "luna"
        };
        let model = resolved.get(tier).context("missing single-model tier")?;
        for config in [&mut base, &mut profile] {
            config["model"] = json!(model);
            config["small_model"] = json!(model);
            let agents = config["agent"].as_object_mut().context("missing agents")?;
            for agent in agents.values_mut() {
                agent["model"] = json!(model);
            }
        }
        // Internal agents otherwise inherit tool defaults outside the bundle.
        for name in [
            "build",
            "plan",
            "general",
            "explore",
            "compaction",
            "title",
            "summary",
            "diff-reviewer",
        ] {
            if profile["agent"].get(name).is_none() {
                profile["agent"][name] = json!({});
            }
            profile["agent"][name]["model"] = json!(model);
        }
    }
    Ok(json!({"opencode.json": base, "opencode-runner.json": profile}))
}
