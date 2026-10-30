import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { parseJsonc, renderOpencode, resolvePromptTier } from '../scripts/render-prompt-tiers.mjs';

const read = (name) => parseJsonc(readFileSync(new URL(`../opencode/${name}`, import.meta.url), 'utf8'));
const manifest = JSON.parse(readFileSync(new URL('../prompt-tiers.json', import.meta.url), 'utf8'));

// The rules agentic-job enforces when a run reads this directory (its
// docs/workflow.md, "Agent configuration repositories"), checked here so a
// refused configuration fails in review rather than in a run. The address
// check stops a real provider host or key landing in this public repository:
// a run replaces both, so the committed values must be inert placeholders.
function checkOpencode(config, runner) {
  const providers = Object.keys(config.provider ?? {});
  assert.equal(providers.length, 1, 'exactly one provider');
  assert.deepEqual(config.enabled_providers, providers, 'enabled_providers names only that provider');
  assert.equal(config.disabled_providers, undefined, 'no disabled_providers');
  const options = config.provider[providers[0]].options ?? {};
  // Only the two keys a run replaces, so no header or other URL can carry a
  // real host or credential past this check.
  assert.deepEqual(Object.keys(options).sort(), ["apiKey", "baseURL"], "options hold only baseURL and apiKey");
  const { baseURL, apiKey } = options;
  assert.match(new URL(baseURL).hostname, /\.invalid$/, 'baseURL is a placeholder');
  assert.equal(apiKey, 'unused', 'apiKey is a placeholder');
  for (const key of ['provider', 'enabled_providers', 'disabled_providers']) {
    assert.equal(runner[key], undefined, `runner profile leaves ${key} alone`);
  }
  const prefix = `${providers[0]}/`;
  const models = [config.model, config.small_model, runner.model, runner.small_model,
    ...[config, runner].flatMap((c) => Object.values(c.agent ?? {}).map((a) => a.model))].filter(Boolean);
  for (const model of models) {
    assert.ok(model.startsWith(prefix) && Object.hasOwn(config.provider[providers[0]].models, model.slice(prefix.length)),
      `${model} is a model the provider defines`);
  }
}

test('opencode configuration meets the wrapper rules', () => {
  checkOpencode(read('opencode.json'), read('opencode-runner.json'));
});

test('GPT-6 reasoning models request encrypted reasoning for stateless turns', () => {
  const models = read('opencode.json').provider.praxis.models;
  for (const name of ['gpt-6.1-sol', 'gpt-6-astra', 'gpt-6-luna']) {
    assert.deepEqual(models[name].options?.include, ['reasoning.encrypted_content'], name);
  }
});

test('every model carries its list price, so opencode reports session cost', () => {
  for (const [name, model] of Object.entries(read('opencode.json').provider.praxis.models)) {
    assert.deepEqual(Object.keys(model.cost ?? {}).sort(), ['cache_read', 'cache_write', 'input', 'output'], name);
    assert.ok(Object.values(model.cost).every((v) => typeof v === 'number' && v > 0), `${name} prices are positive`);
  }
});

test('the check refuses configurations a run would refuse or that leak an address', () => {
  const config = read('opencode.json');
  const runner = read('opencode-runner.json');
  const edit = (f) => { const c = structuredClone(config); f(c); return c; };
  const [name] = Object.keys(config.provider);
  for (const bad of [
    edit((c) => { c.provider.other = c.provider[name]; }),
    edit((c) => { c.enabled_providers = [name, 'other']; }),
    edit((c) => { c.disabled_providers = ['other']; }),
    edit((c) => { c.provider[name].options.baseURL = 'http://host.example.ts.net:18080/v1'; }),
    edit((c) => { c.provider[name].options.apiKey = 'sk-real'; }),
    edit((c) => { c.model = `${name}/undefined-model`; }),
    edit((c) => { c.agent.plan.model = 'other/model'; }),
    edit((c) => { c.model = `${name}/toString`; }),
    edit((c) => { c.provider[name].options.headers = { Authorization: 'Bearer x' }; }),
  ]) {
    assert.throws(() => checkOpencode(bad, runner));
  }
  assert.throws(() => checkOpencode(config, { ...runner, provider: {} }));
  assert.throws(() => checkOpencode(config, { ...runner, small_model: 'other/x' }));
  assert.throws(() => checkOpencode(config, { ...runner, agent: { general: { model: `${name}/nope` } } }));
});

test('parseJsonc keeps // inside strings', () => {
  assert.deepEqual(parseJsonc('{\n  // note\n  "u": "http://x/y"\n}'), { u: 'http://x/y' });
});

test('prompt tiers resolve only the supported pool and explicit fallback edges', () => {
  const config = read('opencode.json');
  for (const [pool, tier, expected] of [
    ['praxis', 'astra', manifest.pools.praxis.astra.model],
    ['praxis', 'sol', config.model],
    ['praxis', 'terra', config.model],
    ['praxis', 'luna', config.small_model],
  ]) {
    assert.equal(resolvePromptTier(manifest, pool, tier, config), expected, `${pool}/${tier}`);
  }
  const unavailable = structuredClone(manifest);
  unavailable.pools.praxis.astra.model = null;
  unavailable.pools.praxis.sol.model = null;
  assert.equal(resolvePromptTier(unavailable, 'praxis', 'astra', config), config.small_model);
});

test('prompt-tier errors name unsupported selections and malformed fallback paths', () => {
  const config = read('opencode.json');
  for (const { name, pool = 'praxis', tier = 'sol', edit = () => {}, error } of [
    { name: 'unsupported pool', pool: 'unverified', error: /Unsupported prompt-tier pool.*unverified/ },
    { name: 'prototype pool', pool: 'toString', error: /Unsupported prompt-tier pool/ },
    { name: 'unknown tier', tier: 'typo', error: /unknown prompt tier.*typo/ },
    { name: 'prototype tier', tier: 'toString', error: /unknown prompt tier/ },
    { name: 'unknown edge', edit: (m) => { m.pools.praxis.terra.fallback = 'typo'; }, error: /unknown fallback tier.*typo/ },
    { name: 'missing terminal', edit: (m) => { m.pools.praxis.luna.model = null; }, error: /missing terminal model.*luna/ },
    { name: 'unverified model', edit: (m) => { m.pools.praxis.luna.model = `${config.small_model}-unverified`; }, error: /not in the tool catalogue/ },
    { name: 'implicit edge', edit: (m) => { delete m.pools.praxis.sol.fallback; }, error: /explicit model and fallback/ },
    { name: 'self cycle', edit: (m) => { m.pools.praxis.astra.fallback = 'astra'; }, error: /fallback cycle astra -> astra/ },
    { name: 'multi-tier cycle', edit: (m) => { m.pools.praxis.luna.fallback = 'astra'; }, error: /fallback cycle astra -> sol -> luna -> astra/ },
    { name: 'unavailable cycle', edit: (m) => { m.pools.praxis.terra.fallback = 'terra'; }, error: /fallback cycle terra -> terra/ },
  ]) {
    const edited = structuredClone(manifest);
    edit(edited);
    assert.throws(() => resolvePromptTier(edited, pool, tier, config), error, name);
  }
  const missing = structuredClone(config);
  delete missing.provider.praxis.models[config.small_model.split('/')[1]];
  assert.throws(() => resolvePromptTier(manifest, 'praxis', 'luna', missing), /not in the tool catalogue/);
});

test('rendered configuration preserves current effective settings and inputs', () => {
  const base = read('opencode.json');
  const runner = read('opencode-runner.json');
  const before = structuredClone({ manifest, base, runner });
  const expected = { ...base, ...runner, agent: { ...base.agent } };
  for (const [name, agent] of Object.entries(runner.agent)) {
    expected.agent[name] = { ...base.agent[name], ...agent };
  }
  const rendered = renderOpencode(manifest, 'praxis', base, runner);
  assert.deepEqual(rendered, expected, 'no change to model choices, limits, prompts, permissions or placeholders');
  checkOpencode(rendered, {});
  assert.deepEqual({ manifest, base, runner }, before, 'inputs are not modified');
  const changed = structuredClone(manifest);
  changed.roles.agent.general = 'terra';
  assert.equal(renderOpencode(changed, 'praxis', base, runner).agent.general.model, base.model);
  changed.roles.agent.general = 'unknown';
  assert.throws(() => renderOpencode(changed, 'praxis', base, runner), /unknown prompt tier/);
  changed.roles.agent = { typo: 'sol' };
  assert.throws(() => renderOpencode(changed, 'praxis', base, runner), /Unknown opencode role typo/);
});

test('renderer CLI is deterministic and fails without partial output', () => {
  const script = new URL('../scripts/render-prompt-tiers.mjs', import.meta.url);
  const run = (...args) => spawnSync(process.execPath, [script.pathname, ...args], { encoding: 'utf8', timeout: 5000 });
  const first = run('praxis');
  assert.equal(first.status, 0, first.stderr);
  assert.equal(first.stderr, '');
  assert.equal(first.stdout, run('praxis').stdout);
  assert.deepEqual(JSON.parse(first.stdout), renderOpencode(manifest, 'praxis', read('opencode.json'), read('opencode-runner.json')));
  for (const args of [[], ['unknown'], ['praxis', 'extra']]) {
    const result = run(...args);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, /prompt tiers: (Usage:|Unsupported prompt-tier pool)/);
  }
});
