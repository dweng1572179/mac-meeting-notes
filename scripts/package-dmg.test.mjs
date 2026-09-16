import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const app = process.argv[2];
assert(app, 'Pass the already built .app path');
const directory = fs.mkdtempSync(join(tmpdir(), 'meeting-notes-package-test-'));
const output = join(directory, 'release.dmg');
const originalCopy = fs.copyFileSync;
try {
  fs.writeFileSync(output, 'previous verified release');
  fs.copyFileSync = (_source, destination) => {
    fs.writeFileSync(destination, 'incomplete copy');
    throw Object.assign(new Error('Simulated disk full'), { code: 'ENOSPC' });
  };
  syncBuiltinESMExports();
  process.argv = [process.execPath, 'package-dmg.mjs', app, output];
  await assert.rejects(import('./package-dmg.mjs'), { code: 'ENOSPC' });
  assert.equal(fs.readFileSync(output, 'utf8'), 'previous verified release');
  assert.deepEqual(fs.readdirSync(directory), ['release.dmg']);
  console.log('Failed package copy preserves the previous DMG and cleans temporary output.');
} finally {
  fs.copyFileSync = originalCopy;
  syncBuiltinESMExports();
  fs.rmSync(directory, { recursive: true, force: true });
}
