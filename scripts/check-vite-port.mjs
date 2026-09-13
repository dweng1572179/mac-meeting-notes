import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

const vite = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1'], {
  stdio: 'ignore'
});
let served = false;

try {
  const deadline = Date.now() + 5_000;

  while (Date.now() < deadline) {
    try {
      const response = await fetch('http://127.0.0.1:1420/');
      if (response.ok) {
        served = true;
        break;
      }
    } catch {}

    await delay(100);
  }

  if (!served) {
    throw new Error('Vite did not serve the Tauri dev URL on port 1420.');
  }
} finally {
  vite.kill();
  await Promise.race([once(vite, 'exit'), delay(1_000)]);
}
