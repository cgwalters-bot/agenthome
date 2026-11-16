use agenthome::readiness::check;
use serde_json::{Value, json};

fn fixture() -> Value {
    let evidence = json!({"source":"https://example.org/evidence", "sha256":"a".repeat(64),
        "observedAt":1000,"validUntil":600000,"pool":"current-openai","windowStart":0,"windowReset":604800});
    let mut v = json!({"version":1,"pool":"current-openai","noNewSpend":true,
        "window":{"start":0,"reset":604800},"finishBy":500000,
        "capacity":evidence,"reservation":evidence,"enforcement":evidence,
        "delivery":evidence,"cache":evidence,"plan":{}});
    v["capacity"]["basis"] = json!("existing-allocation");
    for (key, amount) in [
        ("allocationMicroUsd", 1000000),
        ("spentMicroUsd", 100000),
        ("otherCommitmentsMicroUsd", 200000),
        ("remainingMicroUsd", 700000),
    ] {
        v["capacity"][key] = json!(amount);
    }
    v["reservation"]["microUsd"] = json!(500000);
    v["reservation"]["id"] = json!("reservation-1");
    v["reservation"]["capacitySha256"] = json!("a".repeat(64));
    v["enforcement"]["reservationId"] = json!("reservation-1");
    v["enforcement"]["interface"] = json!("synthetic-test-only");
    v["enforcement"]["scope"] = json!("all-stages-and-children");
    v["enforcement"]["perCell"] = json!({"microUsd":200,"tokens":200,"turns":2,"seconds":2});
    v["enforcement"]["aggregate"] =
        json!({"microUsd":500000,"tokens":500000,"turns":5000,"seconds":5000});
    v["delivery"]["contract"] = json!("source-opencode-runner-profile");
    v["delivery"]["rolesAndModelsVerified"] = json!(true);
    v["delivery"]["harnessRevision"] = json!("b".repeat(40));
    v["cache"]["isolationVerified"] = json!(true);
    v["cache"]["boundedPrimingVerified"] = json!(true);
    for (phase, count) in [("sizing", 3), ("probes", 3), ("priming", 27), ("cells", 54)] {
        v["plan"][phase] =
            json!({"count":count,"limits":{"microUsd":100,"tokens":100,"turns":1,"seconds":1}});
    }
    bind_plan(&mut v);
    v
}

fn bind_plan(v: &mut Value) {
    let digest = agenthome::eval::manifest_digest(&v["plan"]).unwrap();
    for item in ["reservation", "enforcement", "delivery", "cache"] {
        v[item]["planSha256"] = json!(digest);
    }
}

#[test]
fn consistent_evidence_is_not_spending_authority() {
    let result = check(&fixture(), 1001).unwrap();
    assert_eq!(result["inferenceAuthorized"], false);
    assert_eq!(result["planned"]["microUsd"], 8700);
    assert_eq!(result["remainingMicroUsd"], 700000);
}

#[test]
fn missing_stale_inconsistent_and_unenforceable_fail_closed() {
    let cases = [
        ("/capacity", Value::Null),
        ("/capacity/observedAt", json!(1002)),
        ("/capacity/validUntil", json!(1001)),
        ("/capacity/windowReset", json!(604801)),
        ("/capacity/pool", json!("other")),
        ("/capacity/basis", json!("configured-price")),
        ("/capacity/remainingMicroUsd", json!(900000)),
        ("/capacity/otherCommitmentsMicroUsd", json!(1000001)),
        ("/reservation/microUsd", json!(700001)),
        ("/reservation/capacitySha256", json!("c".repeat(64))),
        ("/enforcement/reservationId", json!("other")),
        ("/enforcement/scope", json!("parent-only")),
        ("/enforcement/aggregate/microUsd", json!(500001)),
        ("/delivery/rolesAndModelsVerified", json!(false)),
        ("/delivery/contract", json!("acp-drive")),
        ("/cache/isolationVerified", json!(false)),
        ("/cache/boundedPrimingVerified", json!(false)),
        ("/window/start", json!(1002)),
        ("/finishBy", json!(1080)),
        ("/plan/cells/count", json!(53)),
        ("/plan/priming/count", json!(28)),
    ];
    for (path, replacement) in cases {
        let mut v = fixture();
        *v.pointer_mut(path).unwrap() = replacement;
        assert!(check(&v, 1001).is_err(), "accepted {path}");
    }
    assert!(check(&fixture(), 4601).is_err(), "accepted stale evidence");
}

#[test]
fn every_phase_and_metric_is_bounded_and_aggregated() {
    let mut extra = fixture();
    extra["plan"]["extraJobs"] = json!({"count":1});
    bind_plan(&mut extra);
    assert!(check(&extra, 1001).is_err());
    for phase in ["sizing", "probes", "priming", "cells"] {
        for metric in ["microUsd", "tokens", "turns", "seconds"] {
            for replacement in [Value::Null, json!(0), json!(u64::MAX)] {
                let mut v = fixture();
                v["plan"][phase]["limits"][metric] = replacement;
                bind_plan(&mut v);
                assert!(check(&v, 1001).is_err(), "accepted {phase}.{metric}");
            }
        }
    }
    for metric in ["microUsd", "tokens", "turns", "seconds"] {
        let mut v = fixture();
        v["enforcement"]["aggregate"][metric] = json!(1);
        assert!(check(&v, 1001).is_err(), "aggregate {metric}");
        let mut v = fixture();
        v["enforcement"]["perCell"][metric] = v["plan"]["cells"]["limits"][metric].clone();
        assert!(check(&v, 1001).is_err(), "priming plus cell {metric}");
    }
}
