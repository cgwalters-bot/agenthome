use std::path::Path;

use agenthome::{read_jsonc, tiers::eval_bundle};
use serde_json::{Value, json};

fn inputs() -> [Value; 4] {
    [
        "prompt-tiers.json",
        "opencode/opencode.json",
        "opencode/opencode-runner.json",
        "roles/opencode/eval-tiered.json",
    ]
    .map(|path| read_jsonc(&Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap())
}

#[test]
fn complete_opt_in_bundles_preserve_prompts_and_permissions() {
    let [tiers, base, runner, tiered] = inputs();
    let originals = [tiers.clone(), base.clone(), runner.clone(), tiered.clone()];
    for (arm, tier) in [("single-strong", "sol"), ("single-cheap", "luna")] {
        let bundle = eval_bundle(&tiers, &base, &runner, &tiered, "praxis", arm).unwrap();
        let model = &tiers["pools"]["praxis"][tier]["model"];
        for (file, original) in [("opencode.json", &base), ("opencode-runner.json", &runner)] {
            let config = &bundle[file];
            assert_eq!(&config["model"], model);
            assert_eq!(&config["small_model"], model);
            assert_eq!(config["default_agent"], original["default_agent"]);
            for (name, agent) in config["agent"].as_object().unwrap() {
                assert_eq!(&agent["model"], model, "{arm}/{name}");
                for key in ["prompt", "permission", "disable", "mode"] {
                    assert_eq!(agent[key], original["agent"][name][key]);
                }
            }
            assert_eq!(config["provider"], original["provider"]);
        }
        assert_eq!(
            bundle["opencode-runner.json"]["agent"]["diff-reviewer"]["model"],
            *model
        );
    }
    let bundle = eval_bundle(&tiers, &base, &runner, &tiered, "praxis", "tiered").unwrap();
    assert_eq!(bundle["opencode.json"], base);
    for (name, agent) in tiered["agent"].as_object().unwrap() {
        assert_eq!(bundle["opencode-runner.json"]["agent"][name], *agent);
    }
    assert_eq!(
        bundle["opencode-runner.json"]["default_agent"],
        tiered["default_agent"]
    );
    assert_eq!([tiers, base, runner, tiered], originals);
}

#[test]
fn resolver_is_shared_and_unknown_selections_fail() {
    let [mut tiers, base, runner, tiered] = inputs();
    assert!(eval_bundle(&tiers, &base, &runner, &tiered, "unknown", "tiered").is_err());
    assert!(eval_bundle(&tiers, &base, &runner, &tiered, "praxis", "typo").is_err());
    tiers["pools"]["praxis"]["sol"]["model"] = Value::Null;
    let bundle = eval_bundle(&tiers, &base, &runner, &tiered, "praxis", "single-strong").unwrap();
    assert_eq!(
        bundle["opencode.json"]["model"],
        tiers["pools"]["praxis"]["luna"]["model"]
    );
    let bundle = eval_bundle(&tiers, &base, &runner, &tiered, "praxis", "tiered").unwrap();
    assert_eq!(
        bundle["opencode-runner.json"]["agent"]["role-plan"]["model"],
        tiers["pools"]["praxis"]["luna"]["model"]
    );
    tiers["pools"]["praxis"]["luna"]["fallback"] = json!("sol");
    assert!(eval_bundle(&tiers, &base, &runner, &tiered, "praxis", "single-cheap").is_err());
}

#[test]
fn cli_prints_bundles_without_installing() {
    let [tiers, base, runner, tiered] = inputs();
    for arm in agenthome::eval::ARMS {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_agenthome"))
            .args(["eval-bundle", arm])
            .output()
            .unwrap();
        assert!(output.status.success());
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            actual,
            agenthome::javascript_order(
                &eval_bundle(&tiers, &base, &runner, &tiered, "praxis", arm).unwrap()
            )
            .unwrap()
        );
    }
}
