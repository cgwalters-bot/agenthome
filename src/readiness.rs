//! Offline consistency checks, not allocation authority or an inference launcher.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

const MAX_AGE_SECONDS: u64 = 3600;
const METRICS: [&str; 4] = ["microUsd", "tokens", "turns", "seconds"];
const CELL_MAX: [u64; 4] = [5_000_000, 200_000, 100, 1800];
const PHASES: [&str; 4] = ["sizing", "probes", "priming", "cells"];

fn number(v: &Value, key: &str) -> Result<u64> {
    v[key]
        .as_u64()
        .with_context(|| format!("{key}: expected unsigned integer"))
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .with_context(|| format!("{key}: missing text"))
}

fn evidence(v: &Value, now: u64, window_start: u64, window_end: u64) -> Result<()> {
    ensure!(
        text(v, "pool")? == "current-openai"
            && number(v, "windowStart")? == window_start
            && number(v, "windowReset")? == window_end,
        "evidence pool/window mismatch"
    );
    ensure!(
        text(v, "source")?.starts_with("https://"),
        "evidence source must be accessible HTTPS"
    );
    let digest = text(v, "sha256")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid evidence sha256"
    );
    let observed = number(v, "observedAt")?;
    ensure!(
        observed <= now && now - observed <= MAX_AGE_SECONDS,
        "stale or future evidence"
    );
    ensure!(
        observed >= window_start && observed < window_end,
        "evidence outside accounting window"
    );
    ensure!(
        number(v, "validUntil")? > now && number(v, "validUntil")? <= window_end,
        "expired evidence or validity beyond reset"
    );
    Ok(())
}

/// Check a dated evidence envelope without fetching links or running any command.
/// A successful result means internally consistent assertions, not verified authority.
pub fn check(v: &Value, now: u64) -> Result<Value> {
    ensure!(number(v, "version")? == 1, "unsupported readiness version");
    ensure!(
        text(v, "pool")? == "current-openai",
        "only the existing current OpenAI allocation is permitted"
    );
    ensure!(v["noNewSpend"] == true, "no-new-spend constraint required");
    let window = &v["window"];
    let start = number(window, "start")?;
    let end = number(window, "reset")?;
    ensure!(
        start <= now && now < end && end - start <= 7 * 86400,
        "invalid/current accounting window required"
    );
    let capacity = &v["capacity"];
    let reservation = &v["reservation"];
    let enforcement = &v["enforcement"];
    for item in [
        capacity,
        reservation,
        enforcement,
        &v["delivery"],
        &v["cache"],
    ] {
        evidence(item, now, start, end)?;
    }
    ensure!(
        text(capacity, "basis")? == "existing-allocation",
        "prices and configured targets are not allocation"
    );
    let allocation = number(capacity, "allocationMicroUsd")?;
    let spent = number(capacity, "spentMicroUsd")?;
    let commitments = number(capacity, "otherCommitmentsMicroUsd")?;
    let remaining = allocation
        .checked_sub(spent)
        .and_then(|n| n.checked_sub(commitments))
        .context("allocation exhausted/inconsistent commitments")?;
    ensure!(
        number(capacity, "remainingMicroUsd")? == remaining,
        "remaining allocation must be net of other commitments"
    );
    let reserved = number(reservation, "microUsd")?;
    ensure!(
        reserved > 0 && reserved <= remaining,
        "reservation exceeds remaining allocation"
    );
    ensure!(
        text(reservation, "id")? == text(enforcement, "reservationId")?,
        "enforcement/reservation mismatch"
    );
    ensure!(
        text(reservation, "capacitySha256")? == text(capacity, "sha256")?,
        "reservation bound to different capacity evidence"
    );
    ensure!(
        text(enforcement, "scope")? == "all-stages-and-children",
        "child-inclusive enforcement required"
    );
    text(enforcement, "interface")?;
    ensure!(
        text(&v["delivery"], "contract")? == "source-opencode-runner-profile"
            && v["delivery"]["rolesAndModelsVerified"] == true,
        "supported source profile role/model execution proof required"
    );
    ensure!(
        v["cache"]["isolationVerified"] == true && v["cache"]["boundedPrimingVerified"] == true,
        "cache isolation and bounded priming proof required"
    );
    let revision = text(&v["delivery"], "harnessRevision")?;
    ensure!(
        revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "full harness revision required"
    );
    let plan = &v["plan"];
    ensure!(
        plan.as_object().is_some_and(
            |p| p.len() == PHASES.len() && PHASES.iter().all(|phase| p.contains_key(*phase))
        ),
        "plan must contain exactly sizing, probes, priming and cells"
    );
    let plan_digest = crate::eval::manifest_digest(plan)?;
    for item in [reservation, enforcement, &v["delivery"], &v["cache"]] {
        ensure!(
            text(item, "planSha256")? == plan_digest,
            "evidence bound to different plan"
        );
    }
    ensure!(
        number(&plan["cells"], "count")? == 54,
        "comparison requires exactly 54 cells"
    );
    let mut totals = [0u64; 4];
    for phase in PHASES {
        let entry = &plan[phase];
        let count = number(entry, "count")?;
        ensure!(
            (1..=54).contains(&count),
            "{phase}: bounded positive count required"
        );
        if phase == "priming" {
            ensure!(
                count == 27,
                "exactly one priming procedure per warm cell required"
            );
        }
        for (i, metric) in METRICS.iter().enumerate() {
            let limit = number(&entry["limits"], metric)?;
            ensure!(
                limit > 0 && limit <= CELL_MAX[i],
                "{phase}.{metric}: exceeds per-cell protocol ceiling"
            );
            totals[i] = totals[i]
                .checked_add(limit.checked_mul(count).context("plan overflow")?)
                .context("plan overflow")?;
            let enforced = number(&enforcement["perCell"], metric)?;
            ensure!(
                enforced >= limit && enforced <= CELL_MAX[i],
                "{metric}: per-cell enforcement does not cover plan"
            );
        }
    }
    // Warm priming is also charged to the corresponding cell, not a separate allowance.
    for (i, metric) in METRICS.iter().enumerate() {
        let combined = number(&plan["priming"]["limits"], metric)?
            .checked_add(number(&plan["cells"]["limits"], metric)?)
            .context("priming overflow")?;
        ensure!(
            combined <= number(&enforcement["perCell"], metric)?,
            "{metric}: priming plus cell exceeds enforced cell limit"
        );
        let aggregate = number(&enforcement["aggregate"], metric)?;
        ensure!(
            totals[i] <= aggregate,
            "{metric}: aggregate enforcement below full plan"
        );
        if i == 0 {
            ensure!(
                aggregate <= reserved,
                "aggregate spending ceiling exceeds reservation"
            );
        }
    }
    ensure!(
        number(v, "finishBy")? > now && number(v, "finishBy")? <= end,
        "plan must finish before reset"
    );
    ensure!(
        totals[3] <= number(v, "finishBy")? - now,
        "plan cannot fit before deadline at serial ceiling"
    );
    for item in [
        capacity,
        reservation,
        enforcement,
        &v["delivery"],
        &v["cache"],
    ] {
        ensure!(
            number(item, "validUntil")? >= number(v, "finishBy")?,
            "evidence expires before plan finishes"
        );
    }
    Ok(
        json!({"status": "evidence-consistent", "inferenceAuthorized": false,
        "remainingMicroUsd": remaining, "planned": {
            "microUsd": totals[0], "tokens": totals[1], "turns": totals[2], "seconds": totals[3]
        }}),
    )
}
