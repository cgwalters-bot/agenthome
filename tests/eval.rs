//! Synthetic offline data only: no historical evidence or measured model usage.

use std::{fs, path::Path, process::Command};

use agenthome::eval::{manifest_digest, summarize, validate_manifest};
use serde_json::{Value, json};

const ARMS: [&str; 3] = ["single-strong", "single-cheap", "tiered"];

struct Fixture {
    manifest: Value,
    runs: Vec<Value>,
}

impl Fixture {
    fn new() -> Self {
        let tasks: Vec<_> = ["fix-a", "implement-b"]
            .map(|id| {
                json!({
                    "id": id, "repository": "fixture/repository",
                    "history": format!("fixture://history/{id}"),
                    "startCommit": "1".repeat(40), "taskText": format!("Synthetic common task {id}"),
                    "checks": {"artifact": format!("fixture://checks/{id}"), "sha256": "2".repeat(64)},
                    "sizingEvidence": format!("fixture://sizing/{id}")
                })
            })
            .into();
        let arms: serde_json::Map<_, _> = ARMS
            .iter()
            .enumerate()
            .map(|(i, arm)| {
                let render = json!({
                    "artifact": format!("fixture://render/{arm}"),
                    "sha256": (i + 5).to_string().repeat(64),
                    "resolvedModels": format!("fixture-resolved-map-{arm}")
                });
                (arm.to_string(), render)
            })
            .collect();
        let manifest = json!({
            "version": 1, "status": "frozen",
            "preflight": {"authorization": "fixture://authorization", "evidence": "fixture://sizing"},
            "tasks": tasks,
            "configuration": {
                "configRevision": "3".repeat(40), "harnessRevision": "4".repeat(40),
                "environment": "fixture-image@immutable", "roleInterface": "fixture://renderer-interface",
                "costBasis": {"artifact": "fixture://prices-usd-v1", "sha256": "8".repeat(64)},
                "arms": arms
            },
            "arms": ARMS, "repetitions": 3,
            "cachePolicies": ["cold", "warm"],
            "limits": {"wallSeconds": 1800, "usd": 5, "turns": 100, "tokens": 200000},
            "decisionRule": "quality-cost-time-v1"
        });
        let digest = manifest_digest(&manifest).unwrap();
        let c = &manifest["configuration"];
        let mut runs = Vec::new();
        for t in manifest["tasks"].as_array().unwrap() {
            for arm in ARMS {
                for policy in ["cold", "warm"] {
                    for repetition in 1..=3 {
                        let id =
                            format!("{}-{arm}-{policy}-{repetition}", t["id"].as_str().unwrap());
                        runs.push(json!({
                            "version": 1, "id": id, "manifestSha256": digest,
                            "runUrl": format!("fixture://runs/{id}"),
                            "artifactUrl": format!("fixture://artifacts/{id}"),
                            "taskId": t["id"], "arm": arm, "cachePolicy": policy,
                            "repetition": repetition, "startCommit": t["startCommit"],
                            "taskText": t["taskText"], "checksSha256": t["checks"]["sha256"],
                            "configRevision": c["configRevision"], "harnessRevision": c["harnessRevision"],
                            "environment": c["environment"], "renderSha256": c["arms"][arm]["sha256"],
                            "resolvedModels": c["arms"][arm]["resolvedModels"], "costBasis": c["costBasis"],
                            "telemetry": {"status": "complete"},
                            "metrics": {
                                "inputTokens": {"value": 100}, "outputTokens": {"value": 20},
                                "cacheReadTokens": {"value": if policy == "warm" { 50 } else { 0 }},
                                "cacheWriteTokens": {"value": 10},
                                "costUsd": {"value": if arm == "single-strong" { 1.0 } else { 0.5 }},
                                "wallSeconds": {"value": 100}
                            },
                            "stageSeconds": {"unavailable": "fixture runner does not expose stages"},
                            "outcome": "passed", "failures": [], "testEvidence": format!("fixture://tests/{id}")
                        }));
                    }
                }
            }
        }
        Self { manifest, runs }
    }

    fn evaluate(&self) -> anyhow::Result<Value> {
        summarize(&self.manifest, &Value::Array(self.runs.clone()))
    }

    fn redigest(&mut self) {
        let digest = manifest_digest(&self.manifest).unwrap();
        for run in &mut self.runs {
            run["manifestSha256"] = json!(digest);
        }
    }

    fn find(&mut self, arm: &str, policy: Option<&str>) -> &mut Value {
        self.runs
            .iter_mut()
            .find(|r| r["arm"] == arm && policy.is_none_or(|p| r["cachePolicy"] == p))
            .unwrap()
    }
}

fn verdicts(result: &Value) -> Vec<&str> {
    result["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["verdict"].as_str().unwrap())
        .collect()
}

fn group<'a>(result: &'a Value, arm: &str, policy: &Value) -> &'a Value {
    result["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["arm"] == arm && &g["cachePolicy"] == policy)
        .unwrap()
}

#[test]
fn checked_in_sample_stays_pending_without_fabricated_evidence() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("eval/manifest.json");
    let manifest: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        summarize(&manifest, &json!([])).unwrap()["status"],
        "pending"
    );
    let error = summarize(&manifest, &Value::Array(Fixture::new().runs)).unwrap_err();
    assert!(error.to_string().contains("before freezing"));
}

/// Digests computed before the port by the JavaScript `manifestDigest`, so
/// recorded runs stay valid.
#[test]
fn manifest_digest_matches_javascript_and_ignores_key_order() {
    let manifest = Fixture::new().manifest;
    assert_eq!(
        manifest_digest(&manifest).unwrap(),
        "c0acd25d70b88e46164d89b0e7fb69d21ed869a514fb8fa43cfb563e757e7a47"
    );
    let mut reordered = serde_json::Map::new();
    for (key, value) in manifest.as_object().unwrap().iter().rev() {
        reordered.insert(key.clone(), value.clone());
    }
    assert_eq!(
        manifest_digest(&Value::Object(reordered)).unwrap(),
        manifest_digest(&manifest).unwrap()
    );
    // Number spelling and JavaScript's array-index keys first.
    let input = json!({"b": 1.0, "a": [2, {"10": 1, "9": 2, "x": 3}]});
    assert_eq!(
        manifest_digest(&input).unwrap(),
        "65211c536e345fb524eaa2859ed4ce7a0ed177fd6a23bfcc9c2db9d50c1199b4"
    );
}

#[test]
fn complete_matrix_aggregates_each_stratum_and_applies_the_rule() {
    let result = Fixture::new().evaluate().unwrap();
    assert_eq!(result["status"], "complete");
    assert_eq!(result["groups"].as_array().unwrap().len(), 6);
    assert_eq!(result["groups"][0]["metrics"]["costUsd"]["value"], 6.0);
    assert_eq!(
        result["groups"][0]["metrics"]["cacheReadTokens"]["value"],
        0.0
    );
    assert_eq!(
        result["groups"][3]["metrics"]["cacheReadTokens"]["value"],
        300.0
    );
    assert_eq!(verdicts(&result), ["eligible", "eligible"]);
}

#[test]
fn missing_and_truncated_telemetry_never_becomes_zero_or_a_partial_total() {
    for status in ["missing", "truncated"] {
        let mut f = Fixture::new();
        let r = f.find("tiered", None);
        r["telemetry"] = json!({"status": status, "reason": "capture interrupted"});
        for name in ["costUsd", "inputTokens", "cacheWriteTokens"] {
            r["metrics"][name] = json!({"value": null, "unavailable": "capture interrupted"});
        }
        let policy = r["cachePolicy"].clone();
        let result = f.evaluate().unwrap();
        let g = group(&result, "tiered", &policy);
        assert_eq!(g["metrics"]["costUsd"]["value"], Value::Null);
        assert_eq!(g["metrics"]["cacheWriteTokens"]["value"], Value::Null);
        assert_eq!(g["metrics"]["wallSeconds"]["value"], 600.0);
        assert_eq!(verdicts(&result), ["eligible", "inconclusive"]);
    }
}

#[test]
fn three_task_matrix_and_exposed_stage_timings_are_supported() {
    let mut f = Fixture::new();
    let mut task = f.manifest["tasks"][0].clone();
    task["id"] = json!("fix-c");
    task["taskText"] = json!("Synthetic third task");
    f.manifest["tasks"].as_array_mut().unwrap().push(task);
    let extra: Vec<_> = f
        .runs
        .iter()
        .filter(|r| r["taskId"] == "fix-a")
        .map(|r| {
            let mut r = r.clone();
            r["id"] = json!(format!("{}-third", r["id"].as_str().unwrap()));
            r["taskId"] = json!("fix-c");
            r["taskText"] = json!("Synthetic third task");
            r["stageSeconds"] = json!({"explore": {"value": 10}, "execute": {"value": 90}});
            r
        })
        .collect();
    f.runs.extend(extra);
    f.redigest();
    assert_eq!(f.runs.len(), 54);
    let result = f.evaluate().unwrap();
    assert_eq!(result["status"], "complete");
    assert_eq!(result["groups"][0]["runs"], 9);
    assert_eq!(verdicts(&result), ["eligible", "eligible"]);
    f.manifest["tasks"].as_array_mut().unwrap().pop();
    f.runs.retain(|r| r["taskId"] != "fix-c");
    let error = f.evaluate().unwrap_err();
    assert!(error.to_string().contains("mixed comparison manifests"));
}

#[test]
fn failures_count_against_quality_and_retain_spending() {
    for outcome in ["failed", "timeout", "harness-error"] {
        let mut f = Fixture::new();
        f.runs[0]["outcome"] = json!(outcome);
        f.runs[0]["failures"] = json!(["independent check not passed"]);
        let result = f.evaluate().unwrap();
        assert_eq!(result["groups"][0]["passed"], 5);
        assert_eq!(result["groups"][0]["metrics"]["costUsd"]["value"], 6.0);
        assert_eq!(result["groups"][0]["failures"][0]["outcome"], outcome);
        assert_eq!(verdicts(&result), ["reject", "reject"]);
    }
}

#[test]
fn cost_and_time_thresholds_apply_to_both_strata_including_the_boundary() {
    let mut f = Fixture::new();
    for r in f.runs.iter_mut().filter(|r| r["arm"] == "tiered") {
        r["metrics"]["costUsd"]["value"] = json!(0.8);
        r["metrics"]["wallSeconds"]["value"] = json!(110);
    }
    assert_eq!(verdicts(&f.evaluate().unwrap())[1], "eligible");
    f.find("tiered", Some("warm"))["metrics"]["wallSeconds"]["value"] = json!(111);
    assert_eq!(verdicts(&f.evaluate().unwrap())[1], "reject");
    let mut expensive = Fixture::new();
    expensive.find("tiered", None)["metrics"]["costUsd"]["value"] = json!(4);
    assert_eq!(verdicts(&expensive.evaluate().unwrap())[1], "reject");
}

#[test]
fn decimal_boundaries_tolerate_only_floating_point_roundoff() {
    let mut f = Fixture::new();
    for r in &mut f.runs {
        let strong = r["arm"] == "single-strong";
        r["metrics"]["costUsd"]["value"] = json!(if strong { 0.05 } else { 0.04 });
        r["metrics"]["wallSeconds"]["value"] = json!(if strong { 0.1 } else { 0.11 });
    }
    assert_eq!(verdicts(&f.evaluate().unwrap()), ["eligible", "eligible"]);
    f.find("tiered", None)["metrics"]["costUsd"]["value"] = json!(0.04 + 1e-10);
    assert_eq!(verdicts(&f.evaluate().unwrap())[1], "reject");
}

#[test]
fn incomplete_matrices_and_zero_denominators_cannot_imply_adoption() {
    let mut f = Fixture::new();
    f.runs.pop();
    assert_eq!(
        verdicts(&f.evaluate().unwrap()),
        ["inconclusive", "inconclusive"]
    );
    let empty = summarize(&f.manifest, &json!([])).unwrap();
    assert_eq!(empty["status"], "incomplete");
    assert_eq!(
        empty["groups"][0]["metrics"]["costUsd"]["value"],
        Value::Null
    );
    let mut zero = Fixture::new();
    for r in &mut zero.runs {
        r["metrics"]["costUsd"]["value"] = json!(0);
    }
    assert_eq!(
        verdicts(&zero.evaluate().unwrap()),
        ["inconclusive", "inconclusive"]
    );
}

#[test]
fn sample_limits_and_policy_changes_require_new_records() {
    for edit in [
        |m: &mut Value| m["limits"]["usd"] = json!(0.1),
        |m: &mut Value| m["tasks"].as_array_mut().unwrap().reverse(),
        |m: &mut Value| m["preflight"]["evidence"] = json!("fixture://new-preflight"),
        |m: &mut Value| m["configuration"]["costBasis"]["sha256"] = json!("9".repeat(64)),
    ] {
        let mut f = Fixture::new();
        edit(&mut f.manifest);
        let error = f.evaluate().unwrap_err();
        assert!(error.to_string().contains("mixed comparison manifests"));
    }
}

#[test]
fn unavailable_cost_basis_stays_inconclusive_and_mixed_basis_is_rejected() {
    let mut f = Fixture::new();
    let basis = json!({"unavailable": "no comparable prices"});
    f.manifest["configuration"]["costBasis"] = basis.clone();
    f.redigest();
    for r in &mut f.runs {
        r["costBasis"] = basis.clone();
        r["metrics"]["costUsd"] = json!({"value": null, "unavailable": "no comparable prices"});
    }
    assert_eq!(
        verdicts(&f.evaluate().unwrap()),
        ["inconclusive", "inconclusive"]
    );
    f.runs[0]["metrics"]["costUsd"] = json!({"value": 0});
    assert!(
        f.evaluate()
            .unwrap_err()
            .to_string()
            .contains("unavailable cost basis")
    );
    let mut mixed = Fixture::new();
    mixed.runs[0]["costBasis"]["sha256"] = json!("9".repeat(64));
    assert!(
        mixed
            .evaluate()
            .unwrap_err()
            .to_string()
            .contains("mixed cost basis")
    );
    // Any truthy marker beside a pinned basis still forbids recorded costs.
    for marker in [json!(" "), json!(true), json!(1)] {
        let mut f = Fixture::new();
        f.manifest["configuration"]["costBasis"]["unavailable"] = marker.clone();
        f.redigest();
        let basis = f.manifest["configuration"]["costBasis"].clone();
        for r in &mut f.runs {
            r["costBasis"] = basis.clone();
        }
        let error = f.evaluate().unwrap_err().to_string();
        assert!(
            error.contains("unavailable cost basis"),
            "{marker}: {error}"
        );
    }
}

#[test]
fn mixed_task_or_config_revisions_are_rejected_rather_than_pooled() {
    for field in [
        "startCommit",
        "taskText",
        "checksSha256",
        "configRevision",
        "harnessRevision",
        "environment",
        "renderSha256",
        "resolvedModels",
        "costBasis",
    ] {
        let mut f = Fixture::new();
        f.runs[0][field] = json!("different");
        let error = f.evaluate().unwrap_err();
        assert!(error.to_string().contains("mixed"), "{field}: {error}");
    }
}

#[test]
fn invalid_records_unavailable_metrics_and_duplicate_cells_are_rejected() {
    let edits: [fn(&mut Fixture); 13] = [
        |f| f.runs.push(f.runs[0].clone()),
        |f| f.runs[0]["metrics"]["costUsd"] = json!({"value": null}),
        |f| {
            f.runs[0]["metrics"]
                .as_object_mut()
                .unwrap()
                .remove("cacheReadTokens");
        },
        |f| f.runs[0]["metrics"]["wallSeconds"]["value"] = json!(-1),
        |f| f.runs[0]["metrics"]["costUsd"]["value"] = json!("1"),
        |f| f.runs[0]["stageSeconds"] = json!({}),
        |f| f.runs[0]["telemetry"] = json!({"status": "truncated"}),
        |f| f.runs[0]["outcome"] = json!("failed"),
        |f| f.runs[0]["artifactUrl"] = json!(""),
        |f| f.runs[0]["repetition"] = json!(4),
        |f| f.runs[0]["repetition"] = json!(1.5),
        |f| f.runs[0]["arm"] = json!("unknown"),
        |f| f.runs = vec![f.runs[0].clone(); 55],
    ];
    for (i, edit) in edits.iter().enumerate() {
        let mut f = Fixture::new();
        edit(&mut f);
        assert!(f.evaluate().is_err(), "edit {i}");
    }
    // A repeated cell is a duplicate even when its repetition is spelled 1.0.
    let mut f = Fixture::new();
    let mut copy = f.runs[0].clone();
    copy["id"] = json!("copy");
    copy["repetition"] = serde_json::from_str("1.0").unwrap();
    f.runs[1] = copy;
    assert!(
        f.evaluate()
            .unwrap_err()
            .to_string()
            .contains("duplicate run or matrix cell")
    );
}

#[test]
fn freezing_requires_evidence_bounded_tasks_and_immutable_dependencies() {
    let edits: [fn(&mut Value); 10] = [
        |m| m["preflight"] = Value::Null,
        |m| {
            m["tasks"].as_array_mut().unwrap().pop();
        },
        |m| {
            let tasks = m["tasks"].as_array().unwrap().clone();
            m["tasks"].as_array_mut().unwrap().extend(tasks);
        },
        |m| m["tasks"][0]["startCommit"] = json!("main"),
        |m| m["tasks"][0]["checks"]["sha256"] = json!("latest"),
        |m| m["configuration"]["arms"]["tiered"]["sha256"] = json!(""),
        |m| m["configuration"]["harnessRevision"] = json!("main"),
        |m| m["repetitions"] = json!(100),
        |m| m["limits"]["usd"] = json!(6),
        |m| m["arms"] = json!(["tiered", "single-strong", "single-cheap"]),
    ];
    for (i, edit) in edits.iter().enumerate() {
        let mut manifest = Fixture::new().manifest;
        edit(&mut manifest);
        assert!(validate_manifest(&manifest).is_err(), "edit {i}");
    }
}

#[test]
fn cli_is_offline_refuses_truncated_or_oversized_input_and_reports_usage() {
    let dir = tempfile::tempdir().unwrap();
    let f = Fixture::new();
    let manifest = dir.path().join("manifest.json");
    let runs = dir.path().join("runs.json");
    fs::write(&manifest, f.manifest.to_string()).unwrap();
    fs::write(&runs, Value::Array(f.runs.clone()).to_string()).unwrap();
    let invoke = |args: &[&Path]| {
        Command::new(env!("CARGO_BIN_EXE_agenthome"))
            .arg("summarize-eval")
            .args(args)
            .output()
            .unwrap()
    };
    let good = invoke(&[&manifest, &runs]);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let output: Value = serde_json::from_slice(&good.stdout).unwrap();
    assert_eq!(output["status"], "complete");
    assert_eq!(
        output,
        agenthome::javascript_order(&f.evaluate().unwrap()).unwrap()
    );
    fs::write(&runs, "[{\"version\":").unwrap();
    assert!(!invoke(&[&manifest, &runs]).status.success());
    fs::write(&runs, " ".repeat(4 * 1024 * 1024 + 1)).unwrap();
    let oversized = invoke(&[&manifest, &runs]);
    assert!(!oversized.status.success());
    assert!(String::from_utf8_lossy(&oversized.stderr).contains("exceeds 4 MiB"));
    let usage = invoke(&[]);
    assert!(!usage.status.success() && usage.stdout.is_empty());
    assert!(String::from_utf8_lossy(&usage.stderr).contains("Usage:"));
}
