// Synthetic offline data only: no historical evidence or measured model usage.
import { manifestDigest } from '../../scripts/summarize-eval.mjs';

export function fixture() {
  const manifest = {
    version: 1, status: 'frozen',
    preflight: { authorization: 'fixture://authorization', evidence: 'fixture://sizing' },
    tasks: ['fix-a', 'implement-b'].map((id) => ({
      id, repository: 'fixture/repository', history: `fixture://history/${id}`,
      startCommit: '1'.repeat(40), taskText: `Synthetic common task ${id}`,
      checks: { artifact: `fixture://checks/${id}`, sha256: '2'.repeat(64) },
      sizingEvidence: `fixture://sizing/${id}`,
    })),
    configuration: {
      configRevision: '3'.repeat(40), harnessRevision: '4'.repeat(40),
      environment: 'fixture-image@immutable', roleInterface: 'fixture://renderer-interface',
      costBasis: { artifact: 'fixture://prices-usd-v1', sha256: '8'.repeat(64) },
      arms: Object.fromEntries(['single-strong', 'single-cheap', 'tiered'].map((arm, i) => [arm, {
        artifact: `fixture://render/${arm}`, sha256: String(i + 5).repeat(64),
        resolvedModels: `fixture-resolved-map-${arm}`,
      }])),
    },
    arms: ['single-strong', 'single-cheap', 'tiered'], repetitions: 3,
    cachePolicies: ['cold', 'warm'],
    limits: { wallSeconds: 1800, usd: 5, turns: 100, tokens: 200000 },
    decisionRule: 'quality-cost-time-v1',
  };
  const runs = manifest.tasks.flatMap((t) => manifest.arms.flatMap((arm) =>
    manifest.cachePolicies.flatMap((cachePolicy) => [1, 2, 3].map((repetition) => {
      const id = `${t.id}-${arm}-${cachePolicy}-${repetition}`;
      const c = manifest.configuration;
      return {
        version: 1, id, manifestSha256: manifestDigest(manifest),
        runUrl: `fixture://runs/${id}`, artifactUrl: `fixture://artifacts/${id}`,
        taskId: t.id, arm, cachePolicy, repetition, startCommit: t.startCommit,
        taskText: t.taskText, checksSha256: t.checks.sha256,
        configRevision: c.configRevision, harnessRevision: c.harnessRevision,
        environment: c.environment, renderSha256: c.arms[arm].sha256,
        resolvedModels: c.arms[arm].resolvedModels, costBasis: structuredClone(c.costBasis),
        telemetry: { status: 'complete' },
        metrics: Object.fromEntries(Object.entries({ inputTokens: 100, outputTokens: 20,
          cacheReadTokens: cachePolicy === 'warm' ? 50 : 0, cacheWriteTokens: 10,
          costUsd: arm === 'single-strong' ? 1 : 0.5, wallSeconds: 100,
        }).map(([name, value]) => [name, { value }])),
        stageSeconds: { unavailable: 'fixture runner does not expose stages' },
        outcome: 'passed', failures: [], testEvidence: `fixture://tests/${id}`,
      };
    }))));
  return { manifest, runs };
}
