import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { summarize, validateManifest, manifestDigest } from '../scripts/summarize-eval.mjs';
import { fixture } from './fixtures/eval.mjs';

const evaluate = ({ manifest, runs }) => summarize(manifest, runs);

test('checked-in sample stays pending without fabricated evidence', () => {
  const m = JSON.parse(readFileSync(new URL('../eval/manifest.json', import.meta.url), 'utf8'));
  assert.equal(summarize(m, []).status, 'pending');
  assert.throws(() => summarize(m, fixture().runs), /before freezing/);
});

test('complete synthetic matrix aggregates each stratum and applies declared rule', () => {
  const result = evaluate(fixture());
  assert.equal(result.status, 'complete');
  assert.equal(result.groups.length, 6);
  assert.equal(result.groups[0].metrics.costUsd.value, 6);
  assert.equal(result.groups[0].metrics.cacheReadTokens.value, 0);
  assert.equal(result.groups[3].metrics.cacheReadTokens.value, 300);
  assert.deepEqual(result.decisions.map((d) => d.verdict), ['eligible', 'eligible']);
});

test('missing and truncated telemetry never becomes zero or a partial total', () => {
  for (const status of ['missing', 'truncated']) {
    const f = fixture();
    const r = f.runs.find((r) => r.arm === 'tiered');
    r.telemetry = { status, reason: 'capture interrupted' };
    for (const name of ['costUsd', 'inputTokens', 'cacheWriteTokens']) {
      r.metrics[name] = { value: null, unavailable: 'capture interrupted' };
    }
    const result = evaluate(f);
    const group = result.groups.find((g) => g.arm === 'tiered' && g.cachePolicy === r.cachePolicy);
    assert.equal(group.metrics.costUsd.value, null);
    assert.equal(group.metrics.cacheWriteTokens.value, null);
    assert.equal(group.metrics.wallSeconds.value, 600);
    assert.equal(result.decisions[1].verdict, 'inconclusive');
    assert.equal(result.decisions[0].verdict, 'eligible');
  }
});

test('three-task matrix and exposed stage timings are supported', () => {
  const f = fixture();
  const task = structuredClone(f.manifest.tasks[0]);
  task.id = 'fix-c';
  task.taskText = 'Synthetic third task';
  f.manifest.tasks.push(task);
  const extra = f.runs.filter((r) => r.taskId === 'fix-a').map((r) => ({
    ...structuredClone(r), id: `${r.id}-third`, taskId: task.id, taskText: task.taskText,
    stageSeconds: { explore: { value: 10 }, execute: { value: 90 } },
  }));
  f.runs.push(...extra);
  for (const r of f.runs) r.manifestSha256 = manifestDigest(f.manifest);
  assert.equal(f.runs.length, 54);
  const result = evaluate(f);
  assert.equal(result.status, 'complete');
  assert.equal(result.groups[0].runs, 9);
  assert.ok(result.decisions.every((d) => d.verdict === 'eligible'));
  f.manifest.tasks.pop();
  f.runs = f.runs.filter((r) => r.taskId !== task.id);
  assert.throws(() => evaluate(f), /mixed comparison manifests/);
});

test('failures, timeouts and harness errors count against quality and retain spending', () => {
  for (const outcome of ['failed', 'timeout', 'harness-error']) {
    const f = fixture();
    Object.assign(f.runs[0], { outcome, failures: ['independent check not passed'] });
    const result = evaluate(f);
    assert.equal(result.groups[0].passed, 5);
    assert.equal(result.groups[0].metrics.costUsd.value, 6);
    assert.equal(result.groups[0].failures[0].outcome, outcome);
    assert.ok(result.decisions.every((d) => d.verdict === 'reject'));
  }
});

test('cost and time thresholds apply to both cache strata including boundary', () => {
  const f = fixture();
  for (const r of f.runs.filter((r) => r.arm === 'tiered')) {
    r.metrics.costUsd.value = 0.8;
    r.metrics.wallSeconds.value = 110;
  }
  assert.equal(evaluate(f).decisions[1].verdict, 'eligible');
  f.runs.find((r) => r.arm === 'tiered' && r.cachePolicy === 'warm').metrics.wallSeconds.value = 111;
  assert.equal(evaluate(f).decisions[1].verdict, 'reject');
  const expensive = fixture();
  expensive.runs.find((r) => r.arm === 'tiered').metrics.costUsd.value = 4;
  assert.equal(evaluate(expensive).decisions[1].verdict, 'reject');
});

test('incomplete matrices and zero baseline denominators cannot imply adoption', () => {
  const f = fixture();
  f.runs.pop();
  assert.ok(evaluate(f).decisions.every((d) => d.verdict === 'inconclusive'));
  const empty = summarize(f.manifest, []);
  assert.equal(empty.groups[0].metrics.costUsd.value, null);
  const zero = fixture();
  for (const r of zero.runs) r.metrics.costUsd.value = 0;
  assert.ok(evaluate(zero).decisions.every((d) => d.verdict === 'inconclusive'));
});

test('sample, limits and policy changes require new comparison records', () => {
  for (const edit of [
    (m) => { m.limits.usd = 0.1; },
    (m) => { m.tasks.reverse(); },
    (m) => { m.preflight.evidence = 'fixture://new-preflight'; },
    (m) => { m.configuration.costBasis.sha256 = '9'.repeat(64); },
  ]) {
    const f = fixture(); edit(f.manifest);
    assert.throws(() => evaluate(f), /mixed comparison manifests/);
  }
  const f = fixture();
  const reordered = Object.fromEntries(Object.entries(f.manifest).reverse());
  assert.equal(manifestDigest(reordered), manifestDigest(f.manifest));
});

test('unavailable cost basis stays inconclusive and incompatible run basis is rejected', () => {
  const f = fixture();
  f.manifest.configuration.costBasis = { unavailable: 'no comparable prices' };
  for (const r of f.runs) {
    r.manifestSha256 = manifestDigest(f.manifest);
    r.costBasis = { unavailable: 'no comparable prices' };
    r.metrics.costUsd = { value: null, unavailable: 'no comparable prices' };
  }
  assert.ok(evaluate(f).decisions.every((d) => d.verdict === 'inconclusive'));
  f.runs[0].metrics.costUsd = { value: 0 };
  assert.throws(() => evaluate(f), /unavailable cost basis/);
  const incompatible = fixture();
  incompatible.runs[0].costBasis.sha256 = '9'.repeat(64);
  assert.throws(() => evaluate(incompatible), /mixed cost basis/);
});

test('decimal threshold boundaries tolerate only floating-point roundoff', () => {
  const f = fixture();
  for (const r of f.runs) {
    r.metrics.costUsd.value = r.arm === 'single-strong' ? 0.05 : 0.04;
    r.metrics.wallSeconds.value = r.arm === 'single-strong' ? 0.1 : 0.11;
  }
  assert.ok(evaluate(f).decisions.every((d) => d.verdict === 'eligible'));
  f.runs.find((r) => r.arm === 'tiered').metrics.costUsd.value += 1e-10;
  assert.equal(evaluate(f).decisions[1].verdict, 'reject');
});

test('mixed task/config revisions and provenance are rejected rather than pooled', () => {
  for (const field of ['startCommit', 'taskText', 'checksSha256', 'configRevision', 'harnessRevision',
    'environment', 'renderSha256', 'resolvedModels', 'costBasis']) {
    const f = fixture();
    f.runs[0][field] = 'different';
    assert.throws(() => evaluate(f), /mixed/, field);
  }
});

test('invalid records, unavailable metrics and duplicate cells are rejected', () => {
  for (const edit of [
    (f) => f.runs.push(f.runs[0]),
    (f) => { f.runs[0].metrics.costUsd = { value: null }; },
    (f) => { delete f.runs[0].metrics.cacheReadTokens; },
    (f) => { f.runs[0].metrics.wallSeconds.value = -1; },
    (f) => { f.runs[0].metrics.costUsd.value = Infinity; },
    (f) => { f.runs[0].stageSeconds = {}; },
    (f) => { f.runs[0].telemetry = { status: 'truncated' }; },
    (f) => { f.runs[0].outcome = 'failed'; },
    (f) => { f.runs[0].artifactUrl = ''; },
    (f) => { f.runs[0].repetition = 4; },
    (f) => { f.runs[0].arm = 'unknown'; },
    (f) => { f.runs = Array(55).fill(f.runs[0]); },
  ]) {
    const f = fixture(); edit(f);
    assert.throws(() => evaluate(f));
  }
});

test('freezing requires evidence, bounded tasks and immutable dependencies', () => {
  for (const edit of [
    (m) => { m.preflight = null; },
    (m) => { m.tasks.pop(); },
    (m) => { m.tasks.push(...m.tasks); },
    (m) => { m.tasks[0].startCommit = 'main'; },
    (m) => { m.tasks[0].checks.sha256 = 'latest'; },
    (m) => { m.configuration.arms.tiered.sha256 = ''; },
    (m) => { m.configuration.harnessRevision = 'main'; },
    (m) => { m.repetitions = 100; },
    (m) => { m.limits.usd = 6; },
  ]) {
    const { manifest } = fixture(); edit(manifest);
    assert.throws(() => validateManifest(manifest));
  }
});

test('CLI is offline, rejects truncated JSON and oversized input, and reports usage', () => {
  const dir = mkdtempSync(join(tmpdir(), 'agenthome-eval-'));
  const cli = new URL('../scripts/summarize-eval.mjs', import.meta.url).pathname;
  const invoke = (...args) => spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8' });
  try {
    const f = fixture();
    const manifest = join(dir, 'manifest.json');
    const runs = join(dir, 'runs.json');
    writeFileSync(manifest, JSON.stringify(f.manifest));
    writeFileSync(runs, JSON.stringify(f.runs));
    const good = invoke(manifest, runs);
    assert.equal(good.status, 0, good.stderr);
    assert.equal(JSON.parse(good.stdout).status, 'complete');
    writeFileSync(runs, '[{"version":');
    assert.equal(invoke(manifest, runs).status, 1);
    writeFileSync(runs, ' '.repeat(4 * 1024 * 1024 + 1));
    assert.match(invoke(manifest, runs).stderr, /exceeds 4 MiB/);
    assert.match(invoke().stderr, /usage:/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
