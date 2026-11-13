//! Synthetic declarations only; none are measured capacity or enforcement proof.

use agenthome::readiness::check;
use serde_json::{Value, json};

const NOW: u64 = 1_800_000_000;

fn fixture() -> Value {
    let artifact =
        json!({"artifact": "https://example.org/immutable-evidence", "sha256": "a".repeat(64)});
    json!({
        "version": 1, "pool": "openai", "authorization": artifact,
        "capacityEvidence": artifact, "reservationEvidence": artifact,
        "harnessRevision": "b".repeat(40), "observedAt": NOW - 60,
        "windowStart": NOW - 3600, "windowEnd": NOW + 200_000, "validUntil": NOW + 120_000,
        "allocationUsdMicros": 400_000_000, "spentUsdMicros": 20_000_000,
        "otherCommitmentsUsdMicros": 50_000_000, "reservationUsdMicros": 280_000_000,
        "cells": 54, "overhead": {"usdMicros": 10_000_000, "tokens": 1000, "turns": 10, "wallSeconds": 1000},
        "enforcement": {"evidence": artifact,
            "aggregate": {"usdMicros": 280_000_000, "tokens": 10_801_000, "turns": 5410, "wallSeconds": 98_200},
            "perCell": {"usdMicros": 5_000_000, "tokens": 200_000, "turns": 100, "wallSeconds": 1800},
            "allChildren": true, "phases": ["sizing", "probes", "priming", "cells"],
            "admission": "before-request-hard-cap", "externalDeadline": true, "automaticTopups": false}
    })
}

#[test]
fn consistent_is_not_inference_authorization() {
    let result = check(&fixture(), NOW).unwrap();
    assert_eq!(result["status"], "evidence-consistent");
    assert_eq!(result["inferenceAuthorized"], false);
    assert_eq!(result["remainingUsdMicros"], 330_000_000);
    // The entire worst-case time budget must fit, including the boundary.
    let mut value = fixture();
    value["validUntil"] = json!(NOW + 98_200);
    check(&value, NOW).unwrap();
    assert!(check(&value, NOW + 1).is_err());
}

#[test]
fn bad_or_missing_evidence_fails_closed() {
    let cases = [
        ("/version", json!(2)),
        ("/pool", json!("another-pool")),
        ("/observedAt", json!(NOW + 1)),
        ("/observedAt", json!(NOW - 86_401)),
        ("/windowStart", json!(NOW)),
        ("/validUntil", json!(NOW)),
        ("/validUntil", json!(NOW + 200_001)),
        ("/windowEnd", json!(NOW)),
        ("/allocationUsdMicros", json!(0)),
        ("/spentUsdMicros", json!(u64::MAX)),
        ("/otherCommitmentsUsdMicros", json!(400_000_000)),
        ("/reservationUsdMicros", json!(330_000_001)),
        ("/reservationUsdMicros", json!(279_999_999)),
        ("/cells", json!(53)),
        ("/overhead/tokens", json!(0)),
        ("/overhead/tokens", json!(u64::MAX)),
        ("/enforcement/aggregate/turns", json!(5409)),
        ("/enforcement/perCell/usdMicros", json!(5_000_001)),
        ("/enforcement/perCell/tokens", json!(200_001)),
        ("/enforcement/perCell/turns", json!(101)),
        ("/enforcement/perCell/wallSeconds", json!(1801)),
        ("/enforcement/allChildren", json!(false)),
        ("/enforcement/externalDeadline", json!(false)),
        ("/enforcement/admission", json!("after-reply")),
        ("/enforcement/automaticTopups", json!(true)),
        ("/enforcement/phases", json!(["cells"])),
        ("/harnessRevision", json!("main")),
        ("/capacityEvidence/sha256", json!("not-a-digest")),
        (
            "/capacityEvidence/artifact",
            json!("https://example.org/evidence?secret=value"),
        ),
        ("/capacityEvidence/artifact", json!("https://")),
        (
            "/capacityEvidence/artifact",
            json!("http://example.org/evidence"),
        ),
        ("/authorization", Value::Null),
        ("/reservationEvidence", Value::Null),
        ("/enforcement/evidence", Value::Null),
        ("/enforcement/aggregate/tokens", json!(1.5)),
    ];
    for (path, replacement) in cases {
        let mut value = fixture();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(check(&value, NOW).is_err(), "accepted {path}: {value}");
    }
    let mut value = fixture();
    value["enforcement"]["typo"] = json!(true);
    assert!(check(&value, NOW).is_err());
    for key in fixture().as_object().unwrap().keys() {
        let mut value = fixture();
        value.as_object_mut().unwrap().remove(key);
        assert!(check(&value, NOW).is_err(), "accepted missing {key}");
    }
}

#[test]
fn cli_rejects_pending_evidence_without_inference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("evidence.json");
    std::fs::write(&path, b"{}").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_agenthome"))
        .arg("check-eval-readiness")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("readiness evidence"));
    std::fs::write(
        &path,
        vec![b' '; agenthome::eval::MAX_INPUT_BYTES as usize + 1],
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_agenthome"))
        .arg("check-eval-readiness")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
}
