use std::path::Path;

use agenthome::{read_jsonc, tiers::render_opencode};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

fn read(name: &str) -> Value {
    read_jsonc(&Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).unwrap()
}

/// The host of an `http(s)://` address, without user information or port.
fn hostname(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    Some(host.split(':').next().unwrap_or(host))
}

/// The rules agentic-job enforces when a run reads this directory (its
/// docs/workflow.md, "Agent configuration repositories"), checked here so a
/// refused configuration fails in review rather than in a run. The address
/// check stops a real provider host or key landing in this public repository:
/// a run replaces both, so the committed values must be inert placeholders.
fn check_opencode(config: &Value, runner: &Value) -> Result<()> {
    let providers = config
        .get("provider")
        .and_then(Value::as_object)
        .filter(|providers| providers.len() == 1)
        .context("exactly one provider")?;
    let (name, provider) = providers.iter().next().expect("one provider");
    ensure!(
        config.get("enabled_providers") == Some(&json!([name])),
        "enabled_providers names only that provider"
    );
    ensure!(
        config.get("disabled_providers").is_none(),
        "no disabled_providers"
    );
    // Only the two keys a run replaces, so no header or other URL can carry a
    // real host or credential past this check.
    let options = provider.get("options").cloned().unwrap_or(json!({}));
    let mut keys: Vec<_> = options
        .as_object()
        .context("options is an object")?
        .keys()
        .collect();
    keys.sort();
    ensure!(
        keys == ["apiKey", "baseURL"],
        "options hold only baseURL and apiKey"
    );
    ensure!(
        options["baseURL"]
            .as_str()
            .and_then(hostname)
            .is_some_and(|host| host.ends_with(".invalid")),
        "baseURL is a placeholder"
    );
    ensure!(options["apiKey"] == "unused", "apiKey is a placeholder");
    for key in ["provider", "enabled_providers", "disabled_providers"] {
        ensure!(
            runner.get(key).is_none(),
            "runner profile leaves {key} alone"
        );
    }
    let mut models = Vec::new();
    for source in [config, runner] {
        models.extend([source.get("model"), source.get("small_model")]);
        if let Some(agents) = source.get("agent").and_then(Value::as_object) {
            models.extend(agents.values().map(|agent| agent.get("model")));
        }
    }
    let defined = &provider["models"];
    for model in models.into_iter().flatten().filter(|m| !m.is_null()) {
        let model = model.as_str().context("model is a string")?;
        ensure!(
            model
                .strip_prefix(&format!("{name}/"))
                .is_some_and(|id| defined.get(id).is_some()),
            "{model} is a model the provider defines"
        );
    }
    Ok(())
}

#[test]
fn opencode_configuration_meets_the_wrapper_rules() {
    check_opencode(
        &read("opencode/opencode.json"),
        &read("opencode/opencode-runner.json"),
    )
    .unwrap();
}

#[test]
fn reasoning_models_request_encrypted_reasoning_for_stateless_turns() {
    let config = read("opencode/opencode.json");
    for name in ["gpt-6.1-sol", "gpt-6-astra", "gpt-6-luna"] {
        assert_eq!(
            config["provider"]["praxis"]["models"][name]["options"]["include"],
            json!(["reasoning.encrypted_content"]),
            "{name}"
        );
    }
}

#[test]
fn every_model_carries_its_list_price_so_opencode_reports_cost() {
    let config = read("opencode/opencode.json");
    for (name, model) in config["provider"]["praxis"]["models"].as_object().unwrap() {
        let cost = model["cost"].as_object().unwrap();
        let mut keys: Vec<_> = cost.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["cache_read", "cache_write", "input", "output"],
            "{name}"
        );
        assert!(
            cost.values()
                .all(|v| v.as_f64().is_some_and(|price| price > 0.0)),
            "{name} prices are positive"
        );
    }
}

#[test]
fn the_check_refuses_configurations_a_run_would_refuse_or_that_leak_an_address() {
    let config = read("opencode/opencode.json");
    let runner = read("opencode/opencode-runner.json");
    let edits: [fn(&mut Value); 9] = [
        |c| c["provider"]["other"] = c["provider"]["praxis"].clone(),
        |c| c["enabled_providers"] = json!(["praxis", "other"]),
        |c| c["disabled_providers"] = json!(["other"]),
        |c| {
            c["provider"]["praxis"]["options"]["baseURL"] =
                json!("http://host.example.ts.net:18080/v1")
        },
        |c| c["provider"]["praxis"]["options"]["apiKey"] = json!("sk-real"),
        |c| c["model"] = json!("praxis/undefined-model"),
        |c| c["agent"]["plan"]["model"] = json!("other/model"),
        |c| c["model"] = json!("praxis/toString"),
        |c| c["provider"]["praxis"]["options"]["headers"] = json!({"Authorization": "Bearer x"}),
    ];
    for (i, edit) in edits.iter().enumerate() {
        let mut bad = config.clone();
        edit(&mut bad);
        assert!(check_opencode(&bad, &runner).is_err(), "edit {i}");
    }
    let runners: [fn(&mut Value); 3] = [
        |r| r["provider"] = json!({}),
        |r| r["small_model"] = json!("other/x"),
        |r| r["agent"] = json!({"general": {"model": "praxis/nope"}}),
    ];
    for (i, edit) in runners.iter().enumerate() {
        let mut bad = runner.clone();
        edit(&mut bad);
        assert!(check_opencode(&config, &bad).is_err(), "runner edit {i}");
    }
}

#[test]
fn the_rendered_configuration_meets_the_wrapper_rules() {
    let rendered = render_opencode(
        &read("prompt-tiers.json"),
        "praxis",
        &read("opencode/opencode.json"),
        &read("opencode/opencode-runner.json"),
    )
    .unwrap();
    check_opencode(&rendered, &json!({})).unwrap();
}
