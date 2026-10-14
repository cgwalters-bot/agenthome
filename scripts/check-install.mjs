import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { parseManifest } from './install-tools.mjs';

const installer = fileURLToPath(new URL('./install-tools.mjs', import.meta.url));
const tools = parseManifest(readFileSync(new URL('../tools.toml', import.meta.url), 'utf8'));
function install() {
  const output = execFileSync(process.execPath, [installer], {
    encoding: 'utf8', timeout: 900_000, maxBuffer: 1024 * 1024,
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  process.stdout.write(output);
  return output.trim().split('\n');
}

install();
assert.deepEqual(install(), tools.filter(t => !t.dependency_only).map(t => `${t.name}: present`),
  'Second installation must leave all tools present without reinstalling');
