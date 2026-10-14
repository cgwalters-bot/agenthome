import { test } from 'node:test';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync, existsSync, rmSync, readdirSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseManifest, meetsVersion, installTools, installArchive, run } from '../scripts/install-tools.mjs';

const manifest = readFileSync(new URL('../tools.toml', import.meta.url), 'utf8');
const tools = parseManifest(manifest);

test('manifest parses and rejects unpinned packages and unsupported syntax', () => {
  assert.equal(tools.length, 3);
  for (const text of ['', manifest.replace('ast-outline==1.9.0', 'ast-outline'),
    manifest.replace('method = "uv"', 'method = "shell"'),
    manifest.replace('member = "uv-x86_64-unknown-linux-gnu/uv"', 'member = "../uv"'),
    manifest.replace('package = "@ast-grep/cli"', 'package = "@ast-grep/cli@latest"'),
    manifest.replace('dependency_only = true', 'dependency_only = "true"'),
    manifest + '\n[unsupported]\n', manifest.replace('name = "uv"', 'name = "ast-grep"')]) {
    assert.throws(() => parseManifest(text));
  }
});

test('minimum versions use numeric comparison and reject ambiguous/prerelease output', () => {
  for (const [output, expected] of [['ast-grep 0.50.0', true], ['ast-grep 0.51.0\n', true],
    ['0.9.0', false], ['0.50.0-beta', false], ['nonsense', false], ['1.0.0', true]]) {
    assert.equal(meetsVersion(output, '0.50.0'), expected, output);
  }
});

function harness(initial = {}) {
  const versions = new Map(Object.entries(initial));
  const calls = [];
  const lines = [];
  const execute = (command, args, env) => {
    calls.push([command, args]);
    assert.equal(env.PATH, '/prefix/bin:/usr/bin');
    assert.equal(env.UV_TOOL_BIN_DIR, '/prefix/bin');
    if (args[0] === '--version') {
      if (!versions.has(command)) throw new Error('missing');
      return Buffer.from(`${command} ${versions.get(command)}`);
    }
    if (command === 'npm') {
      assert.deepEqual(args, ['install', '--global', '--prefix', '/prefix', '@ast-grep/cli@0.50.0']);
      versions.set('ast-grep', '0.50.0');
    } else {
      assert.deepEqual([command, args], ['uv', ['tool', 'install', '--force', 'ast-outline==1.9.0']]);
      versions.set('ast-outline', '1.9.0');
    }
  };
  const archive = t => versions.set(t.name, t.minimum_version);
  return { calls, lines, versions, options: { bin: '/prefix/bin', env: { PATH: '/usr/bin' }, execute, archive, log: l => lines.push(l) } };
}

test('missing tools bootstrap uv; second run is present-only without installing', () => {
  const h = harness();
  assert.equal(installTools(tools, h.options), true);
  assert.deepEqual(h.lines, ['uv: installed', 'ast-outline: installed', 'ast-grep: installed']);
  h.lines.length = 0;
  h.calls.length = 0;
  assert.equal(installTools(tools, { ...h.options, archive: () => assert.fail('unexpected install') }), true);
  assert.deepEqual(h.lines, ['ast-outline: present', 'ast-grep: present']);
  assert.equal(h.calls.length, 2);
});

test('old commands upgrade, newer commands skip and unnecessary uv is not checked', () => {
  const h = harness({ 'ast-outline': '1.8.0', uv: '0.13.1', 'ast-grep': '0.51.0' });
  assert.equal(installTools(tools, h.options), true);
  assert.deepEqual(h.lines, ['uv: present', 'ast-outline: installed', 'ast-grep: present']);
  h.calls.length = 0;
  assert.equal(installTools(tools, h.options), true);
  assert.equal(h.calls.some(([command]) => command === 'uv'), false);
});

test('dependency failure fails the run but independent tools still install', () => {
  const h = harness();
  h.options.archive = t => {
    if (t.name === 'uv') throw new Error('download failed');
    h.versions.set(t.name, t.minimum_version);
  };
  assert.equal(installTools(tools, h.options), false);
  assert.deepEqual(h.lines, ['uv: failed (download failed)', 'ast-outline: failed (uv dependency unavailable)', 'ast-grep: installed']);
});

test('old ast-grep upgrades through npm and npm failure fails the run', () => {
  const h = harness({ 'ast-outline': '1.9.0', 'ast-grep': '0.49.0' });
  assert.equal(installTools(tools, h.options), true);
  assert.deepEqual(h.lines, ['ast-outline: present', 'ast-grep: installed']);
  h.versions.delete('ast-grep');
  h.lines.length = 0;
  const execute = h.options.execute;
  h.options.execute = (command, args, env) => {
    if (command === 'npm') throw new Error('registry unavailable');
    return execute(command, args, env);
  };
  assert.equal(installTools(tools, h.options), false);
  assert.deepEqual(h.lines, ['ast-outline: present', 'ast-grep: failed (registry unavailable)']);
});

test('post-install version verification is mandatory', () => {
  const h = harness({ 'ast-outline': '1.9.0' });
  h.options.execute = (command, args) => {
    if (command === 'ast-outline') return Buffer.from('ast-outline 1.9.0');
    if (command === 'npm') return;
    throw new Error('missing');
  };
  assert.equal(installTools(tools, h.options), false);
  assert.match(h.lines[1], /failed.*minimum version/);
});

test('archives verify bytes before extracting, publish selected member and clean staging', () => {
  const root = mkdtempSync(join(tmpdir(), 'agenthome-test-'));
  const bin = join(root, 'bin');
  const bytes = Buffer.from('test archive');
  const digest = createHash('sha256').update(bytes).digest('hex');
  try {
    for (const format of ['zip', 'tar.gz']) {
      let extracted = false;
      let download;
      const t = { ...tools[2], command: 'ast-grep', member: 'ast-grep', format, sha256: digest };
      const execute = (command, args) => {
        if (command === 'curl') {
          download = args[args.indexOf('--output') + 1];
          writeFileSync(download, bytes);
        } else {
          extracted = true;
          assert.deepEqual([command, args], format === 'zip'
            ? ['unzip', ['-p', download, 'ast-grep']]
            : ['tar', ['-xOf', download, 'ast-grep']]);
          return Buffer.from('test binary');
        }
      };
      assert.throws(() => installArchive({ ...t, sha256: '0'.repeat(64) }, bin, {}, execute), /checksum/);
      assert.equal(extracted, false);
      assert.equal(existsSync(download), false);
      installArchive(t, bin, {}, execute);
      assert.equal(extracted, true);
      assert.equal(readFileSync(join(bin, 'ast-grep'), 'utf8'), 'test binary');
      assert.deepEqual(readdirSync(bin), ['ast-grep']);
      assert.equal(existsSync(download), false);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('real archive extraction publishes executable bytes and preserves old binary on failure', () => {
  const root = mkdtempSync(join(tmpdir(), 'agenthome-archive-test-'));
  try {
    const member = join(root, 'ast-grep');
    writeFileSync(member, 'fixture binary');
    for (const format of ['zip', 'tar.gz']) {
      const archive = join(root, `fixture.${format}`);
      if (format === 'zip') execFileSync('zip', ['-j', archive, member]);
      else execFileSync('tar', ['-czf', archive, '-C', root, 'ast-grep']);
      const bytes = readFileSync(archive);
      const t = { ...tools[2], command: 'ast-grep', member: 'ast-grep', format, sha256: createHash('sha256').update(bytes).digest('hex') };
      const execute = (command, args, env) => {
        if (command === 'curl') writeFileSync(args[args.indexOf('--output') + 1], bytes);
        else return run(command, args, env);
      };
      const bin = join(root, 'bin');
      installArchive(t, bin, process.env, execute);
      assert.equal(readFileSync(join(bin, 'ast-grep'), 'utf8'), 'fixture binary');
      assert.equal(statSync(join(bin, 'ast-grep')).mode & 0o777, 0o755 & ~process.umask());
      assert.throws(() => installArchive({ ...t, member: 'missing' }, bin, process.env, execute));
      assert.equal(readFileSync(join(bin, 'ast-grep'), 'utf8'), 'fixture binary');
      assert.deepEqual(readdirSync(bin), ['ast-grep']);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
