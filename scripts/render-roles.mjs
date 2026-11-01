import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseJsonc, resolvePromptTier } from './render-prompt-tiers.mjs';

// One template per role, rendered for both tools under the same names. The
// prefix keeps them clear of opencode's built-in plan and explore agents, of
// Claude Code's Explore and Plan, and of agentic-job's embedded diff-reviewer.
export const ROLES = ['explore', 'plan', 'execute-lead', 'worker', 'review'];
export const PREFIX = 'role-';
export const POOL = 'praxis';
export const TEMPLATE_DIR = 'roles/templates';
export const CLAUDE_DIR = 'roles/claude/agents';
export const OPENCODE_PROFILE = 'roles/opencode/eval-tiered.json';
export const STANDING_RULES = 'opencode/AGENTS.md';
export const CLAUDE_RULES = 'opencode/CLAUDE.md';
const OPENCODE_SCHEMA = 'https://opencode.ai/config.json';

// A role prompt is read in full by every session that loads it.
export const MAX_TEMPLATE_BYTES = 4096;

// What each access level may do. Read-only roles keep the shell for the
// structural tools, as opencode-runner.json's explore does, and rely on their
// prompt to stay read-only; leaf roles cannot delegate, so no subagent can
// start an unbounded chain of others.
const ACCESS = {
  'read-only': {
    permission: { edit: 'deny', bash: 'allow', task: 'deny' },
    tools: 'Read, Grep, Glob, Bash',
  },
  edit: {
    permission: { edit: 'allow', bash: 'allow', task: 'deny' },
    tools: 'Read, Grep, Glob, Bash, Edit, Write',
  },
  // Claude Code subagents cannot start subagents, so the lead only works as
  // the main agent there, with every tool.
  lead: {
    permission: { edit: 'allow', bash: 'allow', task: 'allow' },
    tools: null,
    mode: 'primary',
  },
};

const TEMPLATE_KEYS = ['description', 'tier', 'access'];

// Only the `key: value` lines these templates use, not general YAML.
export function parseTemplate(name, text) {
  const where = `${TEMPLATE_DIR}/${name}.md`;
  if (Buffer.byteLength(text) > MAX_TEMPLATE_BYTES) {
    throw new Error(`${where}: larger than ${MAX_TEMPLATE_BYTES} bytes`);
  }
  const match = /^---\n([\s\S]*?)\n---\n\n([\s\S]+)$/.exec(text);
  if (!match) throw new Error(`${where}: expected frontmatter, a blank line and a body`);
  const meta = {};
  for (const line of match[1].split('\n')) {
    const i = line.indexOf(': ');
    const key = line.slice(0, i);
    if (i < 0 || !TEMPLATE_KEYS.includes(key) || Object.hasOwn(meta, key)) {
      throw new Error(`${where}: unexpected frontmatter line ${JSON.stringify(line)}`);
    }
    meta[key] = line.slice(i + 2).trim();
  }
  for (const key of TEMPLATE_KEYS) {
    if (!meta[key]) throw new Error(`${where}: missing ${key}`);
  }
  if (!Object.hasOwn(ACCESS, meta.access)) {
    throw new Error(`${where}: access must be one of ${Object.keys(ACCESS).join(', ')}`);
  }
  return { name: PREFIX + name, ...meta, body: match[2].trim() };
}

// Claude Code reads its own model aliases, not the praxis catalogue, and the
// manifest has no verified Claude pool, so its agents inherit the run's model.
function claudeAgent(role) {
  const tools = ACCESS[role.access].tools;
  return ['---', `name: ${role.name}`, `description: ${JSON.stringify(role.description)}`,
    ...(tools ? [`tools: ${tools}`] : []), 'model: inherit', '---', '', role.body, ''].join('\n');
}

function opencodeAgent(role, model) {
  const access = ACCESS[role.access];
  return {
    description: role.description,
    mode: access.mode ?? 'subagent',
    model,
    prompt: role.body,
    permission: structuredClone(access.permission),
  };
}

const read = (root, path) => readFileSync(join(root, path), 'utf8');

// Every file the renderer owns, path to content.
export function renderRoles(root) {
  const manifest = parseJsonc(read(root, 'prompt-tiers.json'));
  const catalogue = parseJsonc(read(root, 'opencode/opencode.json'));
  const roles = ROLES.map((name) => parseTemplate(name, read(root, `${TEMPLATE_DIR}/${name}.md`)));
  const extra = readdirSync(join(root, TEMPLATE_DIR)).filter((f) => !ROLES.includes(f.replace(/\.md$/, '')));
  if (extra.length) throw new Error(`${TEMPLATE_DIR}: unknown templates ${extra.join(', ')}`);

  const files = new Map();
  const profile = { $schema: OPENCODE_SCHEMA, default_agent: PREFIX + 'execute-lead', agent: {} };
  for (const role of roles) {
    files.set(`${CLAUDE_DIR}/${role.name}.md`, claudeAgent(role));
    profile.agent[role.name] = opencodeAgent(role, resolvePromptTier(manifest, POOL, role.tier, catalogue));
  }
  files.set(OPENCODE_PROFILE, `${JSON.stringify(profile, null, 2)}\n`);
  // agentic-job's Claude adapter reads only CLAUDE.md; it carries the
  // standing rules unchanged and no role text.
  files.set(CLAUDE_RULES, read(root, STANDING_RULES));
  return files;
}

// Paths that differ from the render: changed, missing, or left in the
// generated Claude directory by a renamed role.
export function drift(root, files) {
  const changed = [];
  for (const [path, content] of files) {
    let current;
    try { current = read(root, path); } catch { current = null; }
    if (current !== content) changed.push(path);
  }
  let present = [];
  try { present = readdirSync(join(root, CLAUDE_DIR)); } catch { /* rendered below */ }
  for (const name of present) {
    if (!files.has(`${CLAUDE_DIR}/${name}`)) changed.push(`${CLAUDE_DIR}/${name}`);
  }
  return changed.sort();
}

// Write only what drifted, so a second render changes nothing.
export function writeRoles(root, files) {
  const changed = drift(root, files);
  for (const path of changed) {
    if (files.has(path)) {
      mkdirSync(dirname(join(root, path)), { recursive: true });
      writeFileSync(join(root, path), files.get(path));
    } else {
      rmSync(join(root, path));
    }
  }
  return changed;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = fileURLToPath(new URL('../', import.meta.url));
  try {
    const args = process.argv.slice(2);
    if (args.length > 1 || (args.length === 1 && args[0] !== '--check')) {
      throw new Error('Usage: node scripts/render-roles.mjs [--check]');
    }
    const files = renderRoles(root);
    if (args[0] === '--check') {
      const changed = drift(root, files);
      if (changed.length) {
        throw new Error(`out of date, run just render-roles: ${changed.join(', ')}`);
      }
    } else {
      for (const path of writeRoles(root, files)) process.stdout.write(`rendered ${path}\n`);
    }
  } catch (error) {
    process.stderr.write(`roles: ${error.message}\n`);
    process.exitCode = 1;
  }
}
