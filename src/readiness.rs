//! Offline consistency checks only: evidence is not spending authority.
//!
//! Threat: launching against stale capacity, double-counted reservations or
//! reply-time/parent-only limits can spend beyond the existing allocation.
//! This opt-in check rejects those declarations; it never contacts a provider.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_AGE_SECONDS: u64 = 86_400;
const CELLS: u64 = 54;
const PHASES: [&str; 4] = ["sizing", "probes", "priming", "cells"];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Artifact {
    artifact: String,
    sha256: String,
}

impl Artifact {
    fn validate(&self) -> Result<()> {
        let authority = self
            .artifact
            .strip_prefix("https://")
            .and_then(|url| url.split('/').next())
            .unwrap_or_default();
        ensure!(
            self.artifact.starts_with("https://")
                && !authority.is_empty()
                && self.artifact.len() <= 2048
                && !self.artifact.contains(['@', '?', '#'])
                && !self.artifact.chars().any(char::is_whitespace),
            "evidence must be a bounded HTTPS artifact link without credentials/query/fragment"
        );
        ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "invalid evidence sha256"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Limits {
    usd_micros: u64,
    tokens: u64,
    turns: u64,
    wall_seconds: u64,
}

impl Limits {
    fn values(self) -> [u64; 4] {
        [self.usd_micros, self.tokens, self.turns, self.wall_seconds]
    }

    fn positive(self) -> bool {
        self.values().iter().all(|n| *n > 0)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Enforcement {
    evidence: Artifact,
    aggregate: Limits,
    per_cell: Limits,
    all_children: bool,
    phases: Vec<String>,
    // Post-reply cost accounting can overshoot; it is not a hard cap.
    admission: String,
    external_deadline: bool,
    automatic_topups: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Readiness {
    version: u32,
    pool: String,
    authorization: Artifact,
    capacity_evidence: Artifact,
    reservation_evidence: Artifact,
    harness_revision: String,
    observed_at: u64,
    window_start: u64,
    window_end: u64,
    valid_until: u64,
    allocation_usd_micros: u64,
    spent_usd_micros: u64,
    other_commitments_usd_micros: u64,
    reservation_usd_micros: u64,
    cells: u64,
    overhead: Limits,
    enforcement: Enforcement,
}

/// `now` is the current Unix time supplied by the CLI (injected only in tests).
/// A successful result is not permission to infer: delivery/cache/sample gates
/// and independent authentication of the linked evidence remain external.
pub fn check(value: &Value, now: u64) -> Result<Value> {
    let r: Readiness = serde_json::from_value(value.clone()).context("readiness evidence")?;
    ensure!(
        r.version == 1 && r.pool == "openai",
        "expected version 1 and existing openai pool"
    );
    for artifact in [
        &r.authorization,
        &r.capacity_evidence,
        &r.reservation_evidence,
        &r.enforcement.evidence,
    ] {
        artifact.validate()?;
    }
    ensure!(
        r.harness_revision.len() == 40
            && r.harness_revision
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "pin full harness revision"
    );
    ensure!(
        r.window_start <= r.observed_at
            && r.observed_at <= now
            && now - r.observed_at <= MAX_AGE_SECONDS
            && now < r.valid_until
            && r.valid_until <= r.window_end,
        "stale, future or inconsistent accounting window/reservation"
    );
    let remaining = r
        .allocation_usd_micros
        .checked_sub(r.spent_usd_micros)
        .and_then(|n| n.checked_sub(r.other_commitments_usd_micros))
        .context("allocation exhausted by spending/other commitments")?;
    ensure!(
        r.reservation_usd_micros > 0 && r.reservation_usd_micros <= remaining,
        "reservation exceeds remaining allocation net of commitments"
    );
    let e = &r.enforcement;
    ensure!(r.cells == CELLS, "reserve all 54 cells");
    ensure!(
        e.all_children
            && e.external_deadline
            && !e.automatic_topups
            && e.admission == "before-request-hard-cap"
            && e.phases.iter().map(String::as_str).eq(PHASES),
        "require pre-request hard caps, external deadline, all children and phases, no top-ups"
    );
    ensure!(
        r.overhead.positive() && e.per_cell.positive() && e.aggregate.positive(),
        "positive sizing/probe/priming and cell budgets required"
    );
    for (value, ceiling) in e
        .per_cell
        .values()
        .into_iter()
        .zip([5_000_000, 200_000, 100, 1800])
    {
        ensure!(value <= ceiling, "per-cell budget exceeds protocol ceiling");
    }
    for ((cell, overhead), aggregate) in e
        .per_cell
        .values()
        .into_iter()
        .zip(r.overhead.values())
        .zip(e.aggregate.values())
    {
        let required = cell
            .checked_mul(CELLS)
            .and_then(|n| n.checked_add(overhead))
            .context("budget arithmetic overflow")?;
        ensure!(
            aggregate >= required,
            "aggregate must cover 54 cells plus sizing/probes/priming"
        );
    }
    ensure!(
        e.aggregate.usd_micros <= r.reservation_usd_micros,
        "aggregate dollars exceed reservation"
    );
    ensure!(
        e.aggregate.wall_seconds <= r.valid_until - now,
        "reservation/window expires before aggregate time budget"
    );
    Ok(
        json!({"status": "evidence-consistent", "inferenceAuthorized": false,
        "remainingUsdMicros": remaining,
        "pending": ["independent evidence authentication", "frozen sample and configurations", "actual role/model execution", "cache isolation and priming"]}),
    )
}
