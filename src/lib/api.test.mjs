import assert from 'node:assert/strict';
import test from 'node:test';
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
