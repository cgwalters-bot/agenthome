//! Offline summary of the three-arm role evaluation (`eval/README.md`).
//! No execution, pricing inference, telemetry repair or exclusion of failed runs.

use std::{collections::HashSet, fs, path::Path};

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const ARMS: [&str; 3] = ["single-strong", "single-cheap", "tiered"];
pub const METRICS: [&str; 6] = [
    "inputTokens",
    "outputTokens",
    "cacheReadTokens",
    "cacheWriteTokens",
    "costUsd",
    "wallSeconds",
];
pub const MAX_RUNS: usize = 54;
pub const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;
const LIMITS: [(&str, f64); 4] = [
    ("wallSeconds", 1800.0),
    ("usd", 5.0),
    ("turns", 100.0),
    ("tokens", 200_000.0),
];

fn text(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn hex(value: Option<&Value>, len: usize) -> bool {
    value.and_then(Value::as_str).is_some_and(|value| {
        value.len() == len
            && value
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn revision(value: Option<&Value>) -> bool {
    hex(value, 40)
}

fn digest(value: Option<&Value>) -> bool {
    hex(value, 64)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

/// JavaScript strict equality for the scalar fields compared here, and deep
/// equality for objects: numbers compare by value, not by spelling.
fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| same(l, r))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, l)| right.get(key).is_some_and(|r| same(l, r)))
        }
        _ => left == right,
    }
}

fn strings(value: Option<&Value>, expected: &[&str]) -> bool {
    value.and_then(Value::as_array).is_some_and(|values| {
        values.len() == expected.len() && values.iter().zip(expected).all(|(v, e)| v == e)
    })
}

/// SHA-256 of the manifest's JSON with object keys in canonical order. Array
/// order stays part of the frozen protocol. Matches `JSON.stringify` of the
/// sorted object, so digests recorded before the Rust port stay valid.
pub fn manifest_digest(manifest: &Value) -> Result<String> {
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
            Value::Object(values) => {
                let mut keys: Vec<_> = values.keys().collect();
                keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                Value::Object(
                    keys.into_iter()
                        .map(|key| (key.clone(), canonical(&values[key])))
                        .collect(),
                )
            }
            _ => value.clone(),
        }
    }
    let json = serde_json::to_string(&crate::javascript_order(&canonical(manifest))?)?;
    Ok(Sha256::digest(json)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// A nonnegative value, or null with the reason it is unavailable.
fn metric(value: Option<&Value>, label: &str) -> Result<Option<f64>> {
    let value = value.filter(|value| value.is_object());
    let result = value.and_then(|metric| match metric.get("value") {
        Some(Value::Null) => text(metric.get("unavailable")).then_some(None),
        found => number(found).filter(|&n| n >= 0.0).map(Some),
    });
    result.with_context(|| {
        format!("{label}: expected nonnegative value or null with unavailable reason")
    })
}

pub fn validate_manifest(m: &Value) -> Result<()> {
    ensure!(
        number(m.get("version")) == Some(1.0),
        "unsupported manifest version"
    );
    ensure!(strings(m.get("arms"), &ARMS), "exactly three declared arms");
    ensure!(
        strings(m.get("cachePolicies"), &["cold", "warm"]),
        "declare cold and warm separately"
    );
    ensure!(
        number(m.get("repetitions")) == Some(3.0),
        "three repetitions required"
    );
    ensure!(
        m.get("decisionRule").and_then(Value::as_str) == Some("quality-cost-time-v1"),
        "unknown decision rule"
    );
    for (key, ceiling) in LIMITS {
        let limit = number(m.get("limits").and_then(|limits| limits.get(key)));
        ensure!(
            limit.is_some_and(|limit| limit > 0.0 && limit <= ceiling),
            "invalid limit {key}"
        );
    }
    let tasks = m
        .get("tasks")
        .and_then(Value::as_array)
        .context("tasks must be an array")?;
    let status = m.get("status").and_then(Value::as_str);
    if status == Some("pending") {
        ensure!(
            text(m.get("pending"))
                && tasks.is_empty()
                && m.get("preflight") == Some(&Value::Null)
                && m.get("configuration") == Some(&Value::Null),
            "pending sample must not claim frozen tasks or configuration"
        );
        return Ok(());
    }
    ensure!(status == Some("frozen"), "status must be pending or frozen");
    let preflight = m.get("preflight");
    ensure!(
        text(preflight.and_then(|p| p.get("authorization")))
            && text(preflight.and_then(|p| p.get("evidence"))),
        "authorized sizing preflight required"
    );
    ensure!((2..=3).contains(&tasks.len()), "freeze two or three tasks");
    let mut ids = HashSet::new();
    for task in tasks {
        let id = task.get("id");
        ensure!(
            text(id) && ids.insert(id.and_then(Value::as_str)),
            "unique task id required"
        );
        ensure!(
            text(task.get("repository"))
                && text(task.get("history"))
                && revision(task.get("startCommit")),
            "task needs history and immutable start"
        );
        let checks = task.get("checks");
        ensure!(
            text(task.get("taskText"))
                && digest(checks.and_then(|c| c.get("sha256")))
                && text(checks.and_then(|c| c.get("artifact"))),
            "common task text and pinned independent checks required"
        );
        ensure!(
            text(task.get("sizingEvidence")),
            "task sizing evidence required"
        );
    }
    let c = m
        .get("configuration")
        .filter(|c| c.is_object())
        .context("pin configuration, harness, role interface and environment")?;
    ensure!(
        revision(c.get("configRevision"))
            && revision(c.get("harnessRevision"))
            && text(c.get("roleInterface"))
            && text(c.get("environment")),
        "pin configuration, harness, role interface and environment"
    );
    let basis = c.get("costBasis").and_then(Value::as_object);
    ensure!(
        basis.is_some_and(|basis| if text(basis.get("unavailable")) {
            basis.len() == 1
        } else {
            text(basis.get("artifact")) && digest(basis.get("sha256"))
        }),
        "pin one common cost basis or declare unavailable"
    );
    let arms = c
        .get("arms")
        .and_then(Value::as_object)
        .filter(|arms| arms.len() == ARMS.len() && ARMS.iter().all(|arm| arms.contains_key(*arm)))
        .context("arm render references required")?;
    for arm in ARMS {
        let a = &arms[arm];
        ensure!(
            text(a.get("artifact")) && digest(a.get("sha256")) && text(a.get("resolvedModels")),
            "{arm}: pin rendered artifact and resolved models"
        );
    }
    Ok(())
}

fn validate_run(r: &Value, m: &Value, manifest_sha256: &str) -> Result<()> {
    ensure!(
        number(r.get("version")) == Some(1.0)
            && text(r.get("id"))
            && text(r.get("runUrl"))
            && text(r.get("artifactUrl")),
        "run identity and artifact links required"
    );
    ensure!(
        r.get("manifestSha256").and_then(Value::as_str) == Some(manifest_sha256),
        "mixed comparison manifests"
    );
    let task = m["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| r.get("taskId").is_some_and(|id| same(id, &t["id"])));
    let arm = r.get("arm").and_then(Value::as_str);
    let policy = r.get("cachePolicy").and_then(Value::as_str);
    let repetition = number(r.get("repetition"));
    let repetitions = m["repetitions"].as_f64().unwrap_or(0.0);
    let task = task
        .filter(|_| {
            arm.is_some_and(|arm| ARMS.contains(&arm))
                && policy.is_some_and(|policy| {
                    m["cachePolicies"]
                        .as_array()
                        .is_some_and(|policies| policies.iter().any(|p| p == policy))
                })
                && repetition.is_some_and(|n| n.fract() == 0.0 && n >= 1.0 && n <= repetitions)
        })
        .context("run outside declared matrix")?;
    let field = |key: &str| r.get(key).unwrap_or(&Value::Null);
    ensure!(
        same(field("startCommit"), &task["startCommit"])
            && same(field("taskText"), &task["taskText"])
            && same(field("checksSha256"), &task["checks"]["sha256"]),
        "mixed task revisions"
    );
    let c = &m["configuration"];
    let render = &c["arms"][arm.expect("checked above")];
    ensure!(
        same(field("configRevision"), &c["configRevision"])
            && same(field("harnessRevision"), &c["harnessRevision"])
            && same(field("environment"), &c["environment"])
            && same(field("renderSha256"), &render["sha256"])
            && same(field("resolvedModels"), &render["resolvedModels"]),
        "mixed config revisions, models, environment or cost basis"
    );
    ensure!(
        same(field("costBasis"), &c["costBasis"]),
        "mixed cost basis"
    );
    let telemetry = r.get("telemetry").and_then(|t| t.get("status"));
    ensure!(
        telemetry.and_then(Value::as_str).is_some_and(|status| [
            "complete",
            "missing",
            "truncated"
        ]
        .contains(&status)),
        "telemetry status required"
    );
    if telemetry.and_then(Value::as_str) != Some("complete") {
        ensure!(
            text(r["telemetry"].get("reason")),
            "incomplete telemetry needs reason"
        );
    }
    let id = r["id"].as_str().expect("checked above");
    let metrics = r.get("metrics");
    for name in METRICS {
        metric(metrics.and_then(|m| m.get(name)), &format!("{id}.{name}"))?;
    }
    // Any truthy marker, even beside a pinned artifact, means no comparable cost.
    let unavailable = match c["costBasis"].get("unavailable") {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::String(reason)) => !reason.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(_) => true,
    };
    if unavailable {
        ensure!(
            r["metrics"]["costUsd"].get("value") == Some(&Value::Null),
            "unavailable cost basis requires unavailable cost"
        );
    }
    let stages = r
        .get("stageSeconds")
        .and_then(Value::as_object)
        .context("stage timing record required")?;
    if stages.contains_key("unavailable") {
        ensure!(
            text(stages.get("unavailable")) && stages.len() == 1,
            "invalid unavailable stages"
        );
    } else {
        ensure!(!stages.is_empty(), "empty stage timing is not unavailable");
        for (stage, value) in stages {
            metric(Some(value), &format!("stage {stage}"))?;
        }
    }
    let outcome = r.get("outcome").and_then(Value::as_str);
    ensure!(
        outcome.is_some_and(|o| ["passed", "failed", "timeout", "harness-error"].contains(&o)),
        "invalid outcome"
    );
    let failures = r.get("failures").and_then(Value::as_array);
    ensure!(
        failures.is_some_and(|failures| {
            failures.iter().all(|f| text(Some(f)))
                && (outcome == Some("passed") || !failures.is_empty())
        }),
        "failed run needs failure details"
    );
    ensure!(
        text(r.get("testEvidence")),
        "independent test evidence required"
    );
    Ok(())
}

fn value(run: &Value, name: &str) -> Option<f64> {
    run["metrics"][name]["value"].as_f64()
}

fn total(runs: &[&Value], name: &str) -> Value {
    if runs.is_empty() {
        return json!({"value": null, "unavailable": "No runs recorded"});
    }
    let missing = runs.iter().filter(|r| value(r, name).is_none()).count();
    if missing > 0 {
        return json!({"value": null, "unavailable": format!("{missing} run(s) lack {name}")});
    }
    json!({"value": runs.iter().filter_map(|r| value(r, name)).sum::<f64>()})
}

/// Whether `value` exceeds `baseline * ratio` by more than floating-point
/// roundoff. At most nine terms per group: absorb only binary rounding, not
/// a currency-sized allowance that would hide real threshold violations.
fn above_threshold(value: f64, baseline: f64, ratio: f64) -> bool {
    let threshold = baseline * ratio;
    let tolerance = 32.0 * f64::EPSILON * value.max(threshold);
    value - threshold > tolerance
}

struct Group<'a> {
    cache_policy: &'a str,
    arm: &'static str,
    passed: usize,
    metrics: Map<String, Value>,
}

impl Group<'_> {
    fn metric(&self, name: &str) -> Option<f64> {
        self.metrics[name]["value"].as_f64()
    }
}

fn decide(groups: &[Group], arm: &str, complete: bool, expected: usize) -> Value {
    let pairs: Vec<_> = groups
        .iter()
        .filter(|g| g.arm == "single-strong")
        .map(|base| {
            let candidate = groups
                .iter()
                .find(|g| g.arm == arm && g.cache_policy == base.cache_policy)
                .expect("every arm has every policy");
            (base, candidate)
        })
        .collect();
    let unavailable = |g: &Group| {
        ["costUsd", "wallSeconds"]
            .iter()
            .any(|n| g.metric(n).is_none())
    };
    let (verdict, reason) = if !complete {
        (
            "inconclusive",
            "Incomplete matrix; do not drop or replace failures.",
        )
    } else if pairs
        .iter()
        .any(|(base, candidate)| base.passed != expected || candidate.passed != expected)
    {
        (
            "reject",
            "Quality gate requires every baseline and candidate run to pass.",
        )
    } else if pairs.iter().any(|(base, candidate)| {
        unavailable(base)
            || unavailable(candidate)
            || base.metric("costUsd") == Some(0.0)
            || base.metric("wallSeconds") == Some(0.0)
    }) {
        (
            "inconclusive",
            "Cost/time unavailable or baseline denominator zero.",
        )
    } else if pairs.iter().any(|(base, candidate)| {
        let [cost, time, base_cost, base_time] = [
            candidate.metric("costUsd"),
            candidate.metric("wallSeconds"),
            base.metric("costUsd"),
            base.metric("wallSeconds"),
        ]
        .map(|value| value.expect("checked above"));
        above_threshold(cost, base_cost, 0.8) || above_threshold(time, base_time, 1.1)
    }) {
        (
            "reject",
            "Cost/time threshold not met in both cache strata.",
        )
    } else {
        (
            "eligible",
            "All checks pass; at least 20% cheaper and no more than 10% slower in both cache strata.",
        )
    };
    json!({"arm": arm, "verdict": verdict, "reason": reason})
}

pub fn summarize(m: &Value, runs: &Value) -> Result<Value> {
    validate_manifest(m)?;
    let runs = runs
        .as_array()
        .filter(|runs| runs.len() <= MAX_RUNS)
        .with_context(|| format!("runs must be an array of at most {MAX_RUNS} records"))?;
    if m["status"] == "pending" {
        ensure!(runs.is_empty(), "cannot accept runs before freezing sample");
        return Ok(json!({
            "status": "pending",
            "reason": m["pending"],
            "groups": [],
            "decisions": []
        }));
    }
    let manifest_sha256 = manifest_digest(m)?;
    let mut cells = HashSet::new();
    let mut ids = HashSet::new();
    for r in runs {
        validate_run(r, m, &manifest_sha256)?;
        let cell = (
            r["taskId"].as_str(),
            r["arm"].as_str(),
            r["cachePolicy"].as_str(),
            // Validated as an integer; 1 and 1.0 are the same cell.
            r["repetition"].as_f64().map(|n| n as u64),
        );
        ensure!(
            cells.insert(cell) && ids.insert(r["id"].as_str()),
            "duplicate run or matrix cell"
        );
    }
    let repetitions = m["repetitions"].as_f64().expect("validated") as usize;
    let expected = m["tasks"].as_array().expect("validated").len() * repetitions;
    let policies: Vec<_> = m["cachePolicies"]
        .as_array()
        .expect("validated")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut groups = Vec::new();
    let mut output = Vec::new();
    for &cache_policy in &policies {
        for arm in ARMS {
            let selected: Vec<_> = runs
                .iter()
                .filter(|r| r["cachePolicy"] == cache_policy && r["arm"] == arm)
                .collect();
            let passed = selected.iter().filter(|r| r["outcome"] == "passed").count();
            let failures: Vec<_> = selected
                .iter()
                .filter(|r| r["outcome"] != "passed")
                .map(|r| json!({"id": r["id"], "outcome": r["outcome"], "failures": r["failures"]}))
                .collect();
            let metrics: Map<_, _> = METRICS
                .iter()
                .map(|name| (name.to_string(), total(&selected, name)))
                .collect();
            output.push(json!({
                "cachePolicy": cache_policy,
                "arm": arm,
                "runs": selected.len(),
                "expected": expected,
                "passed": passed,
                "failures": failures,
                "metrics": metrics
            }));
            groups.push(Group {
                cache_policy,
                arm,
                passed,
                metrics,
            });
        }
    }
    let complete = runs.len() == expected * ARMS.len() * policies.len();
    let decisions: Vec<_> = ARMS[1..]
        .iter()
        .map(|arm| decide(&groups, arm, complete, expected))
        .collect();
    Ok(json!({
        "status": if complete { "complete" } else { "incomplete" },
        "manifestSha256": manifest_sha256,
        "groups": output,
        "decisions": decisions
    }))
}

/// Read one bounded JSON input; strict JSON, no comments.
pub fn read_json(path: &Path) -> Result<Value> {
    let size = fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .len();
    ensure!(
        size <= MAX_INPUT_BYTES,
        "{}: input exceeds 4 MiB",
        path.display()
    );
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}
