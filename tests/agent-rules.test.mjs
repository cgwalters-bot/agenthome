import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = new URL('../', import.meta.url);
const read = (path) => readFileSync(new URL(path, root), 'utf8');

const MAX_AGENTS_LINES = 60;

// The standing rules reach every run, so they name no operator or bot login
// (a run must not act for whoever happens to be written here; agenthome#8
// moves the logins into the run's environment) and no frozen homegit tool a
// run cannot call. The `-forge` lookahead lets links to the org through.
const FORBIDDEN = [
  /\bcgwalters(-bot)?\b(?!-forge)/i,
  /bot-land/i,
  /bot-pr/i,
  /bot-operator/i,
  /bot-git/i,
  /bootstrap\.md/i,
  /worker-preamble/i,
];

// Only the `name: value` lines these files use, not general YAML.
function frontmatter(text) {
  const match = /^---\n([\s\S]*?)\n---\n/.exec(text);
  assert.ok(match, 'starts with YAML frontmatter');
  return Object.fromEntries(match[1].split('\n').map((line) => {
    const i = line.indexOf(':');
    return [line.slice(0, i).trim(), line.slice(i + 1).trim()];
  }));
}

// `agents` is opencode/AGENTS.md; `files` maps each path under skills/ to its
// text.
function checkRules(agents, files) {
  const lines = agents.split('\n').length - (agents.endsWith('\n') ? 1 : 0);
  assert.ok(lines <= MAX_AGENTS_LINES, `opencode/AGENTS.md has ${lines} lines, at most ${MAX_AGENTS_LINES}`);
  const skills = new Set();
  for (const [path, text] of Object.entries(files)) {
    const parts = path.split('/');
    if (parts.length !== 2 || parts[1] !== 'SKILL.md') continue;
    const meta = frontmatter(text);
    assert.equal(meta.name, parts[0], `${path}: name matches its directory`);
    assert.ok(meta.description, `${path}: description is non-empty`);
    skills.add(parts[0]);
  }
  for (const [, name] of agents.matchAll(/`([\w-]+)`\s+skill/g)) {
    assert.ok(skills.has(name), `opencode/AGENTS.md names missing skill ${name}`);
  }
  for (const [path, text] of [['opencode/AGENTS.md', agents], ...Object.entries(files).map(([p, t]) => [`skills/${p}`, t])]) {
    for (const pattern of FORBIDDEN) {
      assert.doesNotMatch(text, pattern, `${path} must not match ${pattern}`);
    }
  }
}

function readSkills() {
  const dir = fileURLToPath(new URL('skills/', root));
  return Object.fromEntries(readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => {
      const path = relative(dir, join(entry.parentPath, entry.name));
      return [path, read(`skills/${path}`)];
    }));
}

test('standing rules and skills meet the repository rules', () => {
  const files = readSkills();
  assert.ok(files['ast-explore/SKILL.md'], 'skills/ were read');
  checkRules(read('opencode/AGENTS.md'), files);
});

test('the check refuses each kind of bad input', () => {
  const skill = (name) => `---\nname: ${name}\ndescription: Load when testing.\n---\n\nBody.\n`;
  const good = { 'demo/SKILL.md': skill('demo') };
  const agents = 'Load the `demo` skill when testing.\n';
  checkRules(agents, good);
  const bads = [
    ['x\n'.repeat(MAX_AGENTS_LINES + 1), good],
    [agents, { 'demo/SKILL.md': skill('other') }],
    [agents, { 'demo/SKILL.md': '---\nname: demo\ndescription:\n---\n' }],
    [agents, { 'demo/SKILL.md': 'No frontmatter.\n' }],
    [`${agents}Load the \`missing\` skill too.\n`, good],
  ];
  for (const word of ['cgwalters', 'CGWalters-bot', 'cgwalters-bot', 'bot-land', 'bot-pr', 'bot-operator',
    'bot-git', 'bootstrap.md', 'worker-preamble']) {
    bads.push([`${agents}See ${word} here.\n`, good]);
    bads.push([agents, { ...good, 'demo/notes.md': `Run ${word}.\n` }]);
  }
  for (const [badAgents, badFiles] of bads) {
    assert.throws(() => checkRules(badAgents, badFiles), undefined, JSON.stringify([badAgents, badFiles]));
  }
  checkRules(`${agents}See https://github.com/cgwalters-forge/agenthome.\n`, good);
});
