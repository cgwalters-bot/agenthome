import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

// These configurations use only whole-line comments, not general JSONC.
export function parseJsonc(text) {
  return JSON.parse(text.split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n'));
}

function poolEntries(manifest, pool) {
  if (!Object.hasOwn(manifest.pools ?? {}, pool)) {
    throw new Error(`Unsupported prompt-tier pool ${JSON.stringify(pool)}; supported: ${Object.keys(manifest.pools ?? {}).join(', ')}`);
  }
  return manifest.pools[pool];
}

// Validate every edge, including unused fallbacks behind available models.
// This stops typos and latent cycles before a catalogue change exposes them.
function resolvePool(manifest, pool, config) {
  const entries = poolEntries(manifest, pool);
  const names = Object.keys(entries);
  if (names.length === 0) throw new Error(`Prompt-tier pool ${pool} is empty`);
  for (const [tier, entry] of Object.entries(entries)) {
    if (!entry || !Object.hasOwn(entry, 'model') || !Object.hasOwn(entry, 'fallback') ||
        !(entry.model === null || typeof entry.model === 'string' && entry.model.length > 0) ||
        !(entry.fallback === null || typeof entry.fallback === 'string')) {
      throw new Error(`Pool ${pool}, tier ${tier}: expected explicit model and fallback (string or null)`);
    }
    if (entry.fallback !== null && !Object.hasOwn(entries, entry.fallback)) {
      throw new Error(`Pool ${pool}, tier ${tier}: unknown fallback tier ${JSON.stringify(entry.fallback)}`);
    }
  }

  const resolved = Object.create(null);
  for (const start of names) {
    const path = [];
    let tier = start;
    let model;
    // At most one visit per tier; no retries, network discovery or pool failover.
    while (tier !== null) {
      if (path.includes(tier)) {
        throw new Error(`Pool ${pool}: fallback cycle ${[...path, tier].join(' -> ')}`);
      }
      path.push(tier);
      const entry = entries[tier];
      if (entry.model !== null) {
        const [provider, ...parts] = entry.model.split('/');
        const id = parts.join('/');
        if (!Object.hasOwn(config.provider ?? {}, provider) ||
            !Object.hasOwn(config.provider[provider].models ?? {}, id)) {
          throw new Error(`Pool ${pool}, tier ${tier}: model ${entry.model} is not in the tool catalogue; use null for an unavailable entry`);
        }
        model ??= entry.model;
      }
      tier = entry.fallback;
    }
    if (model === undefined) {
      throw new Error(`Pool ${pool}: missing terminal model on fallback path ${path.join(' -> ')}`);
    }
    resolved[start] = model;
  }
  return resolved;
}

function lookup(resolved, pool, tier) {
  if (!Object.hasOwn(resolved, tier)) {
    throw new Error(`Pool ${pool}: unknown prompt tier ${JSON.stringify(tier)}`);
  }
  return resolved[tier];
}

export function resolvePromptTier(manifest, pool, tier, config) {
  return lookup(resolvePool(manifest, pool, config), pool, tier);
}

// Render one effective unattended-run configuration. Only model selections
// change; runner prompts, permissions and the provider catalogue stay intact.
export function renderOpencode(manifest, pool, base, runner) {
  const resolved = resolvePool(manifest, pool, base);
  const config = structuredClone({ ...base, ...runner });
  config.agent = {};
  for (const name of new Set([...Object.keys(base.agent ?? {}), ...Object.keys(runner.agent ?? {})])) {
    config.agent[name] = structuredClone({ ...base.agent?.[name], ...runner.agent?.[name] });
  }
  for (const key of ['model', 'small_model']) {
    config[key] = lookup(resolved, pool, manifest.roles[key]);
  }
  for (const [name, tier] of Object.entries(manifest.roles.agent)) {
    if (!Object.hasOwn(config.agent, name)) throw new Error(`Unknown opencode role ${name}`);
    config.agent[name].model = lookup(resolved, pool, tier);
  }
  return config;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length !== 3) throw new Error('Usage: node scripts/render-prompt-tiers.mjs POOL');
    const read = (path) => parseJsonc(readFileSync(new URL(path, import.meta.url), 'utf8'));
    const config = renderOpencode(read('../prompt-tiers.json'), process.argv[2],
      read('../opencode/opencode.json'), read('../opencode/opencode-runner.json'));
    process.stdout.write(`${JSON.stringify(config, null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`prompt tiers: ${error.message}\n`);
    process.exitCode = 1;
  }
}
