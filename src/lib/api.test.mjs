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
