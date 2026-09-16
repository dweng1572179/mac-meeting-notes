import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, renameSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';

const [appArgument, outputArgument] = process.argv.slice(2);
if (!appArgument?.endsWith('.app') || !outputArgument?.endsWith('.dmg')) {
  throw new Error('Usage: node scripts/package-dmg.mjs path/to/App.app output.dmg');
}
const app = resolve(appArgument);
const output = resolve(outputArgument);
const temporary = mkdtempSync(join(tmpdir(), 'meeting-notes-package-'));
const volume = join(temporary, 'volume');
const mount = join(temporary, 'mount');
const image = join(temporary, 'Meeting-Notes.dmg');
const run = (command, args) => execFileSync(command, args, { stdio: 'inherit' });
let mounted = false;
let pendingDirectory;
try {
  run('codesign', ['--verify', '--deep', '--strict', app]);
  mkdirSync(volume);
  mkdirSync(mount);
  run('ditto', [app, join(volume, basename(app))]);
  symlinkSync('/Applications', join(volume, 'Applications'));
  // APFS preserves the signed resources without the default HFS+/Finder metadata mutation.
  run('hdiutil', ['create', '-quiet', '-fs', 'APFS', '-format', 'UDZO', '-volname', 'Meeting Notes', '-srcfolder', volume, image]);
  run('hdiutil', ['attach', '-quiet', '-readonly', '-nobrowse', '-mountpoint', mount, image]);
  mounted = true;
  run('codesign', ['--verify', '--deep', '--strict', join(mount, basename(app))]);
  run('hdiutil', ['detach', '-quiet', mount]);
  mounted = false;
  mkdirSync(dirname(output), { recursive: true });
  pendingDirectory = mkdtempSync(join(dirname(output), '.meeting-notes-dmg-'));
  const pending = join(pendingDirectory, 'release.dmg');
  copyFileSync(image, pending);
  renameSync(pending, output);
  console.log(`Verified DMG: ${output}`);
} finally {
  if (mounted) run('hdiutil', ['detach', '-quiet', mount]);
  if (pendingDirectory) rmSync(pendingDirectory, { recursive: true, force: true });
  rmSync(temporary, { recursive: true, force: true });
}
