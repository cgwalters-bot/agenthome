import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

// opencode accepts JSON with whole-line `//` comments; that is all these files
// use, so strip only those rather than carry a JSONC parser.
function parseJsonc(text) {
  return JSON.parse(text.split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n'));
}

const read = (name) => parseJsonc(readFileSync(new URL(`../opencode/${name}`, import.meta.url), 'utf8'));

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
