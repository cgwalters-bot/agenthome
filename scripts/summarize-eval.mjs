import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const ARMS = ['single-strong', 'single-cheap', 'tiered'];
const METRICS = ['inputTokens', 'outputTokens', 'cacheReadTokens', 'cacheWriteTokens', 'costUsd', 'wallSeconds'];
const REVISION = /^[a-f0-9]{40}$/;
const DIGEST = /^[a-f0-9]{64}$/;
const text = (v) => typeof v === 'string' && v.trim().length > 0;
const positive = (v) => Number.isFinite(v) && v > 0;
const requireValue = (ok, message) => assert.ok(ok, message);

// Canonical object-key order; array order remains part of the frozen protocol.
export function manifestDigest(manifest) {
  const canonical = (value) => {
    if (Array.isArray(value)) return value.map(canonical);
    if (value && typeof value === 'object') {
      return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
    }
    return value;
  };
  return createHash('sha256').update(JSON.stringify(canonical(manifest))).digest('hex');
}

function metric(m, label) {
  requireValue(m && (m.value === null ? text(m.unavailable) :
    typeof m.value === 'number' && Number.isFinite(m.value) && m.value >= 0),
  `${label}: expected nonnegative value or null with unavailable reason`);
  return m.value;
}

export function validateManifest(m) {
  requireValue(m.version === 1, 'unsupported manifest version');
  assert.deepEqual(m.arms, ARMS, 'exactly three declared arms');
  assert.deepEqual(m.cachePolicies, ['cold', 'warm'], 'declare cold and warm separately');
  requireValue(m.repetitions === 3, 'three repetitions required');
  requireValue(m.decisionRule === 'quality-cost-time-v1', 'unknown decision rule');
  for (const [key, ceiling] of Object.entries({ wallSeconds: 1800, usd: 5, turns: 100, tokens: 200000 })) {
    requireValue(positive(m.limits?.[key]) && m.limits[key] <= ceiling, `invalid limit ${key}`);
  }
  requireValue(Array.isArray(m.tasks), 'tasks must be an array');
  if (m.status === 'pending') {
    requireValue(text(m.pending) && m.tasks.length === 0 && m.preflight === null && m.configuration === null,
      'pending sample must not claim frozen tasks or configuration');
    return m;
  }
  requireValue(m.status === 'frozen', 'status must be pending or frozen');
  requireValue(text(m.preflight?.authorization) && text(m.preflight?.evidence), 'authorized sizing preflight required');
  requireValue(m.tasks.length >= 2 && m.tasks.length <= 3, 'freeze two or three tasks');
  const ids = new Set();
  for (const t of m.tasks) {
    requireValue(text(t.id) && !ids.has(t.id), 'unique task id required');
    ids.add(t.id);
    requireValue(text(t.repository) && text(t.history) && REVISION.test(t.startCommit), 'task needs history and immutable start');
    requireValue(text(t.taskText) && DIGEST.test(t.checks?.sha256) && text(t.checks?.artifact),
      'common task text and pinned independent checks required');
    requireValue(text(t.sizingEvidence), 'task sizing evidence required');
  }
  const c = m.configuration;
  requireValue(c && REVISION.test(c.configRevision) && REVISION.test(c.harnessRevision) &&
    text(c.roleInterface) && text(c.environment), 'pin configuration, harness, role interface and environment');
  requireValue(c.costBasis && (text(c.costBasis.unavailable) ? Object.keys(c.costBasis).length === 1 :
    text(c.costBasis.artifact) && DIGEST.test(c.costBasis.sha256)), 'pin one common cost basis or declare unavailable');
  assert.deepEqual(Object.keys(c.arms).sort(), [...ARMS].sort(), 'arm render references required');
  for (const arm of ARMS) {
    const a = c.arms[arm];
    requireValue(text(a.artifact) && DIGEST.test(a.sha256) && text(a.resolvedModels),
      `${arm}: pin rendered artifact and resolved models`);
  }
  return m;
}

function validateRun(r, m, digest) {
  requireValue(r.version === 1 && text(r.id) && text(r.runUrl) && text(r.artifactUrl), 'run identity and artifact links required');
  requireValue(r.manifestSha256 === digest, 'mixed comparison manifests');
  const task = m.tasks.find((t) => t.id === r.taskId);
  requireValue(task && ARMS.includes(r.arm) && m.cachePolicies.includes(r.cachePolicy) &&
    Number.isInteger(r.repetition) && r.repetition >= 1 && r.repetition <= m.repetitions, 'run outside declared matrix');
  requireValue(r.startCommit === task.startCommit && r.taskText === task.taskText &&
    r.checksSha256 === task.checks.sha256, 'mixed task revisions');
  const c = m.configuration;
  requireValue(r.configRevision === c.configRevision && r.harnessRevision === c.harnessRevision &&
    r.environment === c.environment && r.renderSha256 === c.arms[r.arm].sha256 &&
    r.resolvedModels === c.arms[r.arm].resolvedModels,
  'mixed config revisions, models, environment or cost basis');
  assert.deepEqual(r.costBasis, c.costBasis, 'mixed cost basis');
  requireValue(['complete', 'missing', 'truncated'].includes(r.telemetry?.status), 'telemetry status required');
  if (r.telemetry.status !== 'complete') requireValue(text(r.telemetry.reason), 'incomplete telemetry needs reason');
  for (const name of METRICS) metric(r.metrics?.[name], `${r.id}.${name}`);
  if (c.costBasis.unavailable) requireValue(r.metrics.costUsd.value === null, 'unavailable cost basis requires unavailable cost');
  requireValue(r.stageSeconds && typeof r.stageSeconds === 'object' && !Array.isArray(r.stageSeconds), 'stage timing record required');
  if (Object.hasOwn(r.stageSeconds, 'unavailable')) {
    requireValue(text(r.stageSeconds.unavailable) && Object.keys(r.stageSeconds).length === 1, 'invalid unavailable stages');
  } else {
    requireValue(Object.keys(r.stageSeconds).length > 0, 'empty stage timing is not unavailable');
    for (const [stage, value] of Object.entries(r.stageSeconds)) metric(value, `stage ${stage}`);
  }
  requireValue(['passed', 'failed', 'timeout', 'harness-error'].includes(r.outcome), 'invalid outcome');
  requireValue(Array.isArray(r.failures) && r.failures.every(text) && (r.outcome === 'passed' || r.failures.length > 0),
    'failed run needs failure details');
  requireValue(text(r.testEvidence), 'independent test evidence required');
}

function total(runs, name) {
  if (runs.length === 0) return { value: null, unavailable: 'No runs recorded' };
  const missing = runs.filter((r) => r.metrics[name].value === null);
  return missing.length ? { value: null, unavailable: `${missing.length} run(s) lack ${name}` } :
    { value: runs.reduce((sum, r) => sum + r.metrics[name].value, 0) };
}

// No execution, pricing inference, telemetry repair or exclusion of failed runs.
export function summarize(manifest, runs) {
  const m = validateManifest(manifest);
  requireValue(Array.isArray(runs) && runs.length <= 54, 'runs must be an array of at most 54 records');
  if (m.status === 'pending') {
    requireValue(runs.length === 0, 'cannot accept runs before freezing sample');
    return { status: 'pending', reason: m.pending, groups: [], decisions: [] };
  }
  const seen = new Set();
  const ids = new Set();
  const digest = manifestDigest(m);
  for (const r of runs) {
    validateRun(r, m, digest);
    const key = JSON.stringify([r.taskId, r.arm, r.cachePolicy, r.repetition]);
    requireValue(!seen.has(key) && !ids.has(r.id), 'duplicate run or matrix cell');
    seen.add(key);
    ids.add(r.id);
  }
  const expected = m.tasks.length * m.repetitions;
  const groups = m.cachePolicies.flatMap((cachePolicy) => ARMS.map((arm) => {
    const selected = runs.filter((r) => r.cachePolicy === cachePolicy && r.arm === arm);
    return { cachePolicy, arm, runs: selected.length, expected,
      passed: selected.filter((r) => r.outcome === 'passed').length,
      failures: selected.filter((r) => r.outcome !== 'passed').map((r) => ({ id: r.id, outcome: r.outcome, failures: r.failures })),
      metrics: Object.fromEntries(METRICS.map((name) => [name, total(selected, name)])) };
  }));
  const complete = runs.length === expected * ARMS.length * m.cachePolicies.length;
  const decisions = ARMS.slice(1).map((arm) => {
    const pairs = m.cachePolicies.map((policy) => [groups.find((g) => g.arm === 'single-strong' && g.cachePolicy === policy),
      groups.find((g) => g.arm === arm && g.cachePolicy === policy)]);
    let verdict = 'eligible';
    let reason = 'All checks pass; at least 20% cheaper and no more than 10% slower in both cache strata.';
    if (!complete) {
      verdict = 'inconclusive'; reason = 'Incomplete matrix; do not drop or replace failures.';
    } else if (pairs.some(([base, candidate]) => base.passed !== expected || candidate.passed !== expected)) {
      verdict = 'reject'; reason = 'Quality gate requires every baseline and candidate run to pass.';
    } else if (pairs.some((pair) => pair.some((g) => ['costUsd', 'wallSeconds'].some((name) => g.metrics[name].value === null)) ||
      pair[0].metrics.costUsd.value === 0 || pair[0].metrics.wallSeconds.value === 0)) {
      verdict = 'inconclusive'; reason = 'Cost/time unavailable or baseline denominator zero.';
    } else if (pairs.some(([base, candidate]) => aboveThreshold(candidate.metrics.costUsd.value, base.metrics.costUsd.value, 0.8) ||
      aboveThreshold(candidate.metrics.wallSeconds.value, base.metrics.wallSeconds.value, 1.1))) {
      verdict = 'reject'; reason = 'Cost/time threshold not met in both cache strata.';
    }
    return { arm, verdict, reason };
  });
  return { status: complete ? 'complete' : 'incomplete', manifestSha256: digest, groups, decisions };
}

function aboveThreshold(value, baseline, ratio) {
  const threshold = baseline * ratio;
  // At most nine terms per group. Only absorb binary floating-point rounding,
  // not a currency-sized allowance that would hide real threshold violations.
  const tolerance = 32 * Number.EPSILON * Math.max(value, threshold);
  return value - threshold > tolerance;
}

function readJson(path) {
  requireValue(statSync(path).size <= 4 * 1024 * 1024, 'input exceeds 4 MiB');
  return JSON.parse(readFileSync(path, 'utf8'));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    requireValue(process.argv.length === 4, 'usage: node scripts/summarize-eval.mjs MANIFEST.json RUNS.json');
    process.stdout.write(`${JSON.stringify(summarize(readJson(process.argv[2]), readJson(process.argv[3])), null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`eval: ${error.message}\n`);
    process.exitCode = 1;
  }
}
