import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { mockIPC } from '@tauri-apps/api/mocks';
import * as api from './api.ts';

test('disposes a listener that registers after its owner is disposed', async () => {
  let finishRegistration;
  const registration = new Promise((resolve) => (finishRegistration = resolve));
  let unlistenCalls = 0;

  const dispose = api.disposeAsyncListener(registration);
  dispose();
  finishRegistration(() => unlistenCalls++);
  await registration;
  await Promise.resolve();

  assert.equal(unlistenCalls, 1);
  dispose();
  assert.equal(unlistenCalls, 1);
});

test('close waits for pending notes before destroying the native window', async () => {
  let finishSave;
  const saveFinished = new Promise((resolve) => (finishSave = resolve));
  const order = [];
  const close = api.createCloseHandler(
    async () => {
      order.push('save-started');
      await saveFinished;
      order.push('save-finished');
    },
    async () => order.push('destroyed')
  );

  const closing = close();
  await Promise.resolve();
  assert.deepEqual(order, ['save-started']);
  finishSave();
  await closing;

  assert.deepEqual(order, ['save-started', 'save-finished', 'destroyed']);
});

test('the prevented close handshake is authorized to destroy only the main window', async () => {
  const capability = JSON.parse(readFileSync(new URL('../../src-tauri/capabilities/default.json', import.meta.url)));
  globalThis.window = {};
  mockIPC((command) => {
    assert.equal(command, 'plugin:window|destroy');
    assert.ok(capability.windows.includes('main'));
    assert.ok(capability.permissions.includes('core:window:allow-destroy'));
  });
  window.__TAURI_INTERNALS__.metadata = { currentWindow: { label: 'main' } };
  try {
    await api.createCloseHandler(async () => {}, api.destroyCurrentWindow)();
  } finally {
    delete globalThis.window;
  }
});
