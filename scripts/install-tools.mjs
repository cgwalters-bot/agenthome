import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, mkdirSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Deliberately a small TOML subset: tool tables and scalar/string-array values.
// Reject unsupported syntax instead of silently interpreting a changed manifest.
export function parseManifest(text) {
  const tools = [];
  let tool;
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    if (line === '[[tools]]') {
      tool = {};
      tools.push(tool);
      continue;
    }
    const match = /^(\w+)\s*=\s*(.*)$/.exec(line);
    if (!tool || !match || Object.hasOwn(tool, match[1])) throw new Error(`Invalid manifest line: ${line}`);
    tool[match[1]] = JSON.parse(match[2]);
  }
  const names = new Set();
  for (const t of tools) {
    if (!/^[a-z][a-z0-9-]*$/.test(t.name) || names.has(t.name) || t.command !== t.name
        || !/^\d+\.\d+\.\d+$/.test(t.minimum_version)
        || !Array.isArray(t.version_args) || !t.version_args.every(a => typeof a === 'string')
        || !['uv', 'npm', 'archive'].includes(t.method)
        || (t.dependency_only !== undefined && typeof t.dependency_only !== 'boolean')) throw new Error('Invalid tool specification');
    names.add(t.name);
    if (t.method === 'archive' && (!/^https:\/\//.test(t.url) || !/^[a-f0-9]{64}$/.test(t.sha256)
        || !['zip', 'tar.gz'].includes(t.format) || !/^[a-zA-Z0-9_./-]+$/.test(t.member)
        || t.member.startsWith('/') || t.member.split('/').includes('..'))) throw new Error('Invalid archive specification');
    if (t.method === 'uv' && t.package !== `${t.name}==${t.minimum_version}`) throw new Error('Unpinned package');
    if (t.method === 'npm' && !/^@[a-z0-9-]+\/[a-z0-9-]+$/.test(t.package)) throw new Error('Invalid npm package');
  }
  if (!tools.length) throw new Error('Empty manifest');
  return tools;
}

export function meetsVersion(output, minimum) {
  const match = /(?:^|\s)(\d+)\.(\d+)\.(\d+)(?=\s|$)/.exec(output.trim());
  if (!match) return false;
  const actual = match.slice(1).map(Number);
  const required = minimum.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    if (actual[i] !== required[i]) return actual[i] > required[i];
  }
  return true;
}

export function run(command, args, env) {
  return execFileSync(command, args, { env, timeout: 180_000, maxBuffer: 128 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
}

export function installArchive(tool, bin, env, execute = run) {
  const temporary = mkdtempSync(join(tmpdir(), 'agenthome-tools-'));
  try {
    const archive = join(temporary, 'archive');
    execute('curl', ['--fail', '--silent', '--show-error', '--location', '--proto', '=https', '--proto-redir', '=https', '--max-time', '120', '--max-filesize', '134217728', '--output', archive, tool.url], env);
    // Stops a substituted/corrupted release artifact before any extraction.
    const digest = createHash('sha256').update(readFileSync(archive)).digest('hex');
    if (digest !== tool.sha256) throw new Error('Release checksum mismatch');
    // Extract only the named binary, never archive paths into the destination.
    const contents = tool.format === 'zip'
      ? execute('unzip', ['-p', archive, tool.member], env)
      : execute('tar', ['-xOf', archive, tool.member], env);
    if (!contents.length) throw new Error('Empty release binary');
    mkdirSync(bin, { recursive: true });
    const staging = mkdtempSync(join(bin, '.install-'));
    try {
      const target = join(staging, tool.command);
      writeFileSync(target, contents, { mode: 0o755 });
      renameSync(target, join(bin, tool.command));
    } finally {
      rmSync(staging, { recursive: true, force: true });
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

export function installTools(tools, { bin, env = process.env, execute = run, archive = installArchive, log = console.log }) {
  const environment = { ...env, PATH: `${bin}:${env.PATH ?? ''}`, UV_TOOL_BIN_DIR: bin,
    UV_TOOL_DIR: join(dirname(bin), 'share', 'agenthome', 'uv-tools') };
  const results = new Map();
  const present = t => {
    try { return meetsVersion(execute(t.command, t.version_args, environment).toString(), t.minimum_version); }
    catch { return false; }
  };
  function ensure(t) {
    if (results.has(t.name)) return results.get(t.name);
    try {
      if (present(t)) {
        log(`${t.name}: present`);
      } else {
        if (t.method === 'uv') {
          const uv = tools.find(t => t.name === 'uv');
          if (!uv || !ensure(uv)) throw new Error('uv dependency unavailable');
          execute('uv', ['tool', 'install', '--force', t.package], environment);
        } else if (t.method === 'npm') {
          execute('npm', ['install', '--global', '--prefix', dirname(bin),
            `${t.package}@${t.minimum_version}`], environment);
        } else {
          archive(t, bin, environment, execute);
        }
        if (!present(t)) throw new Error('Installed command does not meet minimum version');
        log(`${t.name}: installed`);
      }
      results.set(t.name, true);
    } catch (error) {
      // Keep subprocess transcripts out of the one-line summary.
      log(`${t.name}: failed (${error.message.split('\n')[0]})`);
      results.set(t.name, false);
    }
    return results.get(t.name);
  }
  for (const t of tools.filter(t => !t.dependency_only)) ensure(t);
  return [...results.values()].every(Boolean);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.platform !== 'linux' || process.arch !== 'x64') throw new Error('Supported platform: Linux x86_64 only');
    const tools = parseManifest(readFileSync(new URL('../tools.toml', import.meta.url), 'utf8'));
    const bin = join(homedir(), '.local', 'bin');
    if (!installTools(tools, { bin })) process.exitCode = 1;
  } catch (error) {
    console.error(`install-tools: ${error.message}`);
    process.exitCode = 1;
  }
}
