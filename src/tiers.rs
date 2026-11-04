use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value};

fn nullable_string<'a>(entry: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match entry.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if key != "model" || !value.is_empty() => Ok(Some(value)),
        _ => bail!("expected explicit model and fallback (string or null)"),
    }
}

/// Validate every fallback edge, including edges behind available models.
/// A typo must not select an undeclared model, and a latent cycle must not
/// hang resolution when the catalogue changes. Each path visits a tier once.
fn resolve_pool(manifest: &Value, pool: &str, config: &Value) -> Result<HashMap<String, String>> {
    let entries = manifest
        .get("pools")
        .and_then(|pools| pools.get(pool))
        .and_then(Value::as_object)
        .with_context(|| format!("Unsupported prompt-tier pool {pool:?}"))?;
    ensure!(!entries.is_empty(), "Prompt-tier pool {pool} is empty");
    for (tier, entry) in entries {
        nullable_string(entry, "model").with_context(|| format!("Pool {pool}, tier {tier}"))?;
        if let Some(fallback) = nullable_string(entry, "fallback")
            .with_context(|| format!("Pool {pool}, tier {tier}"))?
        {
            ensure!(
                entries.contains_key(fallback),
                "Pool {pool}, tier {tier}: unknown fallback tier {fallback:?}"
            );
        }
    }

    let mut resolved = HashMap::new();
    for start in entries.keys() {
        let mut path = Vec::new();
        let mut seen = HashSet::new();
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
                    config
                        .get("provider")?
                        .get(provider)?
                        .get("models")?
                        .get(id)
                });
                ensure!(
                    configured.is_some(),
                    "Pool {pool}, tier {current}: model {candidate} is not in the tool catalogue; use null for an unavailable entry"
                );
                model.get_or_insert(candidate);
            }
            tier = nullable_string(entry, "fallback")?;
        }
        let model = model.with_context(|| {
            format!(
                "Pool {pool}: missing terminal model on fallback path {}",
                path.join(" -> ")
            )
        })?;
        resolved.insert(start.clone(), model.to_owned());
    }
    Ok(resolved)
}

fn lookup<'a>(resolved: &'a HashMap<String, String>, pool: &str, tier: &str) -> Result<&'a str> {
    resolved
        .get(tier)
        .map(String::as_str)
        .with_context(|| format!("Pool {pool}: unknown prompt tier {tier:?}"))
}

pub fn resolve_prompt_tier(
    manifest: &Value,
    pool: &str,
    tier: &str,
    config: &Value,
) -> Result<String> {
    Ok(lookup(&resolve_pool(manifest, pool, config)?, pool, tier)?.to_owned())
}

fn object(value: &Value) -> Result<&Map<String, Value>> {
    value.as_object().context("expected configuration object")
}

pub fn render_opencode(
    manifest: &Value,
    pool: &str,
    base: &Value,
    runner: &Value,
) -> Result<Value> {
    let resolved = resolve_pool(manifest, pool, base)?;
    let mut config = object(base)?.clone();
    config.extend(object(runner)?.clone());
    let mut agents = Map::new();
    for source in [base, runner] {
        if let Some(source_agents) = source.get("agent") {
            for (name, agent) in object(source_agents)? {
                let target = agents
                    .entry(name.clone())
                    .or_insert_with(|| Value::Object(Map::new()));
                target
                    .as_object_mut()
                    .context("expected agent object")?
                    .extend(object(agent)?.clone());
            }
        }
    }
    let roles = manifest.get("roles").context("missing role assignments")?;
    for key in ["model", "small_model"] {
        let tier = roles[key].as_str().context("expected role tier")?;
        config.insert(key.to_owned(), lookup(&resolved, pool, tier)?.into());
    }
    for (name, tier) in object(&roles["agent"])? {
        let agent = agents
            .get_mut(name)
            .with_context(|| format!("Unknown opencode role {name}"))?;
        agent["model"] = lookup(
            &resolved,
            pool,
            tier.as_str().context("expected agent tier")?,
        )?
        .into();
    }
    config.insert("agent".to_owned(), Value::Object(agents));
    Ok(Value::Object(config))
}
