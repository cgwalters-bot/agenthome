import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { parseJsonc } from '../scripts/render-prompt-tiers.mjs';
import {
  CLAUDE_DIR, CLAUDE_RULES, MAX_TEMPLATE_BYTES, OPENCODE_PROFILE, PREFIX, ROLES, STANDING_RULES, TEMPLATE_DIR,
  drift, parseTemplate, renderRoles, writeRoles,
} from '../scripts/render-roles.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const read = (path) => readFileSync(join(root, path), 'utf8');
const catalogue = parseJsonc(read('opencode/opencode.json'));
const rendered = renderRoles(root);
const profile = JSON.parse(rendered.get(OPENCODE_PROFILE));
const templates = Object.fromEntries(ROLES.map((r) => [r, parseTemplate(r, read(`${TEMPLATE_DIR}/${r}.md`))]));

// The keys of opencode v1.18.35's agent schema
// (packages/core/src/v1/config/agent.ts) and Claude Code's subagent
// frontmatter that the renderer may emit.
const OPENCODE_AGENT_KEYS = ['description', 'mode', 'model', 'prompt', 'permission'];
const OPENCODE_MODES = ['subagent', 'primary', 'all'];
const OPENCODE_ACTIONS = ['ask', 'allow', 'deny'];
const CLAUDE_KEYS = ['name', 'description', 'tools', 'model'];
const CLAUDE_TOOLS = ['Read', 'Grep', 'Glob', 'Bash', 'Edit', 'Write'];
// Names a rendered role must not shadow: opencode's and Claude Code's
// built-in agents, and agentic-job's embedded reviewer.
const RESERVED = ['build', 'plan', 'general', 'explore', 'compaction', 'title', 'summary', 'diff-reviewer',
  'Explore', 'Plan', 'general-purpose'];

function claudeFile(text) {
  const match = /^---\n([\s\S]*?)\n---\n\n([\s\S]*)$/.exec(text);
  assert.ok(match, 'Claude agent has frontmatter and a body');
  const meta = Object.fromEntries(match[1].split('\n').map((line) => {
    const i = line.indexOf(': ');
    return [line.slice(0, i), line.slice(i + 2)];
  }));
  return { meta, body: match[2].trim() };
}

test('each role has the same text, description and access in both forms', () => {
  for (const role of ROLES) {
    const t = templates[role];
    const { meta, body } = claudeFile(rendered.get(`${CLAUDE_DIR}/${PREFIX}${role}.md`));
    const agent = profile.agent[PREFIX + role];
    assert.equal(body, t.body, `${role}: Claude body is the template body`);
    assert.equal(agent.prompt, t.body, `${role}: opencode prompt is the template body`);
    assert.equal(JSON.parse(meta.description), t.description, role);
    assert.equal(agent.description, t.description, role);
    assert.equal(meta.name, PREFIX + role, role);
    const readOnly = t.access === 'read-only';
    assert.equal(agent.permission.edit === 'deny', readOnly, `${role}: opencode edit matches access`);
    assert.equal(!/\b(Edit|Write)\b/.test(meta.tools ?? 'Edit'), readOnly, `${role}: Claude tools match access`);
    assert.equal(agent.permission.task === 'allow', t.access === 'lead', `${role}: only the lead delegates`);
  }
  assert.deepEqual(Object.keys(profile.agent), ROLES.map((r) => PREFIX + r));
  assert.equal(profile.default_agent, `${PREFIX}execute-lead`);
});

// Model identifiers belong in prompt-tiers.json and tool configuration.
const MODEL_PATTERNS = [
  ...Object.keys(catalogue.provider.praxis.models).map((id) => new RegExp(id.replaceAll('.', '\\.'), 'i')),
  /\bpraxis\b/i, /\bgpt-\d/i, /\bclaude-/i, /\b(opus|sonnet|haiku|fable|gemini|mistral)\b/i,
  /\b(astra|sol|terra|luna)\b/i,
];

function checkNoModels(name, text) {
  for (const pattern of MODEL_PATTERNS) assert.doesNotMatch(text, pattern, `${name} must not match ${pattern}`);
}

test('role prompts name no model or tier', () => {
  for (const role of ROLES) checkNoModels(role, templates[role].body);
  for (const [path, text] of rendered) {
    if (path.startsWith(CLAUDE_DIR)) checkNoModels(path, text);
  }
  for (const bad of ['Use gpt-6-luna.', 'Ask Opus.', 'Run on the luna tier.', 'praxis/x', 'claude-opus-5-5']) {
    assert.throws(() => checkNoModels('bad', bad), undefined, bad);
  }
});

test('generated configuration is valid for the pinned tools', () => {
  for (const key of ['provider', 'enabled_providers', 'disabled_providers', 'model', 'small_model']) {
    assert.equal(profile[key], undefined, `the eval profile leaves ${key} to opencode.json`);
  }
  for (const [name, agent] of Object.entries(profile.agent)) {
    assert.ok(!RESERVED.includes(name), `${name} is not reserved`);
    assert.deepEqual(Object.keys(agent), OPENCODE_AGENT_KEYS, name);
    assert.ok(OPENCODE_MODES.includes(agent.mode), name);
    assert.ok(Object.values(agent.permission).every((a) => OPENCODE_ACTIONS.includes(a)), name);
    const [provider, id] = agent.model.split('/');
    assert.ok(Object.hasOwn(catalogue.provider, provider) && Object.hasOwn(catalogue.provider[provider].models, id),
      `${name}: ${agent.model} is in the catalogue`);
  }
  for (const role of ROLES) {
    const { meta } = claudeFile(rendered.get(`${CLAUDE_DIR}/${PREFIX}${role}.md`));
    assert.ok(Object.keys(meta).every((k) => CLAUDE_KEYS.includes(k)), role);
    assert.ok(!RESERVED.includes(meta.name) && !meta.name.includes(':'), role);
    assert.equal(meta.model, 'inherit', role);
    for (const tool of meta.tools?.split(', ') ?? []) assert.ok(CLAUDE_TOOLS.includes(tool), `${role}: ${tool}`);
  }
});

test('standing rules carry no role text, and the Claude copy is rendered', () => {
  assert.equal(rendered.get(CLAUDE_RULES), read(STANDING_RULES));
  const rules = read(STANDING_RULES).replace(/\s+/g, ' ');
  for (const role of ROLES) {
    for (const paragraph of templates[role].body.split(/\n\s*\n/)) {
      const sentence = paragraph.replace(/\s+/g, ' ').split('. ')[0];
      assert.ok(!rules.includes(sentence), `${STANDING_RULES} repeats ${role}: ${sentence}`);
    }
  }
});

test('the checked-in outputs are current', () => {
  assert.deepEqual(drift(root, rendered), []);
  const run = spawnSync(process.execPath, [join(root, 'scripts/render-roles.mjs'), '--check'], { encoding: 'utf8', timeout: 5000 });
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stdout + run.stderr, '');
});

// A scratch copy of the renderer's inputs and outputs.
function scratch() {
  const dir = mkdtempSync(join(tmpdir(), 'roles-'));
  for (const path of ['prompt-tiers.json', 'opencode', 'roles', 'scripts']) {
    cpSync(join(root, path), join(dir, path), { recursive: true });
  }
  return dir;
}

test('a second render changes nothing', () => {
  const dir = scratch();
  try {
    rmSync(join(dir, 'roles/claude'), { recursive: true });
    assert.deepEqual(writeRoles(dir, renderRoles(dir)), [...rendered.keys()].filter((p) => p.startsWith(CLAUDE_DIR)).sort());
    const mtime = statSync(join(dir, OPENCODE_PROFILE)).mtimeMs;
    assert.deepEqual(writeRoles(dir, renderRoles(dir)), []);
    assert.equal(statSync(join(dir, OPENCODE_PROFILE)).mtimeMs, mtime);
  } finally {
    rmSync(dir, { recursive: true });
  }
});

test('check mode refuses each kind of drift', () => {
  const lead = `${CLAUDE_DIR}/${PREFIX}execute-lead.md`;
  for (const { name, edit, error } of [
    { name: 'edited output', edit: (d) => writeFileSync(join(d, OPENCODE_PROFILE), '{}\n'), error: OPENCODE_PROFILE },
    { name: 'missing output', edit: (d) => rmSync(join(d, lead)), error: lead },
    { name: 'stale output', edit: (d) => writeFileSync(join(d, CLAUDE_DIR, 'role-old.md'), 'x\n'), error: 'role-old.md' },
    { name: 'edited template', edit: (d) => writeFileSync(join(d, TEMPLATE_DIR, 'worker.md'), read(`${TEMPLATE_DIR}/worker.md`) + 'More.\n'), error: 'role-worker.md' },
    { name: 'edited rules', edit: (d) => writeFileSync(join(d, STANDING_RULES), 'Changed.\n'), error: CLAUDE_RULES },
    { name: 'changed tier', edit: (d) => writeFileSync(join(d, 'prompt-tiers.json'), read('prompt-tiers.json').replace('"luna": { "model": "praxis/gpt-6-luna"', '"luna": { "model": "praxis/gpt-6-astra"')), error: OPENCODE_PROFILE },
  ]) {
    const dir = scratch();
    try {
      edit(dir);
      const run = spawnSync(process.execPath, [join(dir, 'scripts/render-roles.mjs'), '--check'], { encoding: 'utf8', timeout: 5000 });
      assert.equal(run.status, 1, name);
      assert.ok(run.stderr.includes(error), `${name}: ${run.stderr}`);
      const render = spawnSync(process.execPath, [join(dir, 'scripts/render-roles.mjs')], { encoding: 'utf8', timeout: 5000 });
      assert.equal(render.status, 0, `${name}: ${render.stderr}`);
      assert.deepEqual(drift(dir, renderRoles(dir)), [], `${name}: render repairs it`);
    } finally {
      rmSync(dir, { recursive: true });
    }
  }
});

test('templates are bounded and name a known tier and access', () => {
  const good = read(`${TEMPLATE_DIR}/worker.md`);
  for (const [name, text, error] of [
    ['no frontmatter', 'Body.\n', /expected frontmatter/],
    ['unknown key', good.replace('access: edit', 'model: edit'), /unexpected frontmatter line/],
    ['duplicate key', good.replace('access: edit', 'access: edit\naccess: lead'), /unexpected frontmatter line/],
    ['missing key', good.replace('tier: luna\n', ''), /missing tier/],
    ['unknown access', good.replace('access: edit', 'access: root'), /access must be one of/],
    ['too large', good + 'x'.repeat(MAX_TEMPLATE_BYTES), /larger than/],
  ]) {
    assert.throws(() => parseTemplate('worker', text), error, name);
  }
  const dir = scratch();
  try {
    writeFileSync(join(dir, TEMPLATE_DIR, 'worker.md'), good.replace('tier: luna', 'tier: praxis/gpt-6-luna'));
    assert.throws(() => renderRoles(dir), /unknown prompt tier/);
    writeFileSync(join(dir, TEMPLATE_DIR, 'worker.md'), good);
    writeFileSync(join(dir, TEMPLATE_DIR, 'extra.md'), good);
    assert.throws(() => renderRoles(dir), /unknown templates extra\.md/);
  } finally {
    rmSync(dir, { recursive: true });
  }
});
