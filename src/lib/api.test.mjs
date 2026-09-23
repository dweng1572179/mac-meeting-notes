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

test('meeting questions send the selected folder and question to the native core', async () => {
  globalThis.window = {};
  mockIPC((command, payload) => {
    assert.equal(command, 'ask_meetings');
    assert.deepEqual(payload, { folder: 'Acquisitions', question: 'What is still open?' });
    return {
      answer: 'The rent roll remains open.',
      citations: [
        { sessionId: 'meeting-1', title: 'Harbor review', excerpt: 'Verify the rent roll.' }
      ]
    };
  });
  try {
    const answer = await api.askMeetings('Acquisitions', 'What is still open?');
    assert.equal(answer.citations[0].sessionId, 'meeting-1');
  } finally {
    delete globalThis.window;
  }
});

test('capture health requests the active session and preserves source warnings', async () => {
  globalThis.window = {};
  mockIPC((command, payload) => {
    assert.equal(command, 'recording_health');
    assert.deepEqual(payload, { id: 'active-meeting' });
    return { wallSeconds: 45, warnings: ['System audio has produced no frames.'] };
  });
  try {
    const health = await api.recordingHealth('active-meeting');
    assert.equal(health.wallSeconds, 45);
    assert.deepEqual(health.warnings, ['System audio has produced no frames.']);
  } finally {
    delete globalThis.window;
  }
});

test('question history preserves native scope and includes only the latest four turns', async () => {
  globalThis.window = {};
  const requests = [];
  const history = Array.from({ length: 6 }, (_, i) => ({ question: `Question ${i}`, answer: `Answer ${i}` }));
  mockIPC((command, payload) => { requests.push({ command, payload }); return { answer: 'Synthetic answer', citations: [] }; });
  try {
    await api.askMeeting('exact-id', 'Explain that.', history);
    await api.askMeetings('Course notes', 'Compare those ideas.', history);
    assert.deepEqual(requests, [
      { command: 'ask_meetings', payload: { folder: null, sessionId: 'exact-id', question: 'Explain that.', history: history.slice(-4) } },
      { command: 'ask_meetings', payload: { folder: 'Course notes', question: 'Compare those ideas.', history: history.slice(-4) } }
    ]);
  } finally { delete globalThis.window; }
});

test('single-meeting questions and settings preserve their native scope and values', async () => {
  globalThis.window = {};
  const settings = { language: 'en', vocabulary: 'Darryl, São Paulo', model: 'gpt-4o-transcribe' };
  const requests = [];
  mockIPC((command, payload) => {
    requests.push({ command, payload });
    return command === 'save_transcription_settings' ? settings : { answer: 'Saved decision', citations: [] };
  });
  try {
    await api.askMeeting('exact-id', 'What was decided?');
    assert.deepEqual(await api.saveTranscriptionSettings(settings), settings);
    assert.deepEqual(requests, [
      { command: 'ask_meetings', payload: { folder: null, sessionId: 'exact-id', question: 'What was decided?' } },
      { command: 'save_transcription_settings', payload: { settings } }
    ]);
  } finally { delete globalThis.window; }
});

test('native Markdown export passes content without choosing an arbitrary filesystem path', async () => {
  globalThis.window = {};
  mockIPC((command, payload) => {
    assert.equal(command, 'export_markdown');
    assert.deepEqual(payload, { title: 'Meeting', markdown: '# Original\n\nKeep my words.' });
    return '/Users/example/Downloads/Meeting-unique.md';
  });
  try {
    assert.match(await api.exportMarkdown('Meeting', '# Original\n\nKeep my words.'), /Downloads\/Meeting-unique.md$/);
  } finally { delete globalThis.window; }
});

test('new recordings default to speaker detection while explicit settings remain available', () => {
  assert.equal(api.defaultTranscriptionSettings.model, 'gpt-4o-transcribe-diarize');
});

test('Note edits, suggestion actions, and refresh preserve exact native meeting scope', async () => {
  globalThis.window = {};
  const requests = [];
  mockIPC((command, payload) => { requests.push({ command, payload }); return { id: 'meeting-1' }; });
  try {
    const input = { id: 'meeting-1', title: 'Meeting', context: '', attendees: [], folder: '', originalNotes: 'Legacy', notes: '' };
    await api.saveSession(input);
    await api.applySuggestion('meeting-1', 'title', 'apply');
    await api.applySuggestion('meeting-1', 'participants', 'dismiss');
    await api.refreshInsights('meeting-1');
    assert.deepEqual(requests, [
      { command: 'save_session', payload: { input } },
      { command: 'apply_suggestion', payload: { id: 'meeting-1', key: 'title', action: 'apply' } },
      { command: 'apply_suggestion', payload: { id: 'meeting-1', key: 'participants', action: 'dismiss' } },
      { command: 'refresh_insights', payload: { id: 'meeting-1' } }
    ]);
  } finally { delete globalThis.window; }
});

test('synthetic preview keeps unique meetings isolated through edits, questions, restore and deletion', async () => {
  globalThis.window = { location: { search: '?key&askDelay=0&meetings=2' } };
  try {
    const firstLibrary = await api.bootstrap();
    const original = firstLibrary.sessions[0];
    const first = await api.createSession({ title: 'Synthetic first', context: 'First context', attendees: [] });
    const second = await api.createSession({ title: 'Synthetic second', context: 'Second context', attendees: [] });
    assert.notEqual(first.id, second.id);
    await api.saveSession({ ...first, notes: 'First private note', folder: 'First folder' });
    await api.saveSession({ ...second, notes: 'Second private note', folder: 'Second folder' });
    let library = await api.bootstrap();
    assert.equal(library.sessions.find(({ id }) => id === first.id).notes, 'First private note');
    assert.equal(library.sessions.find(({ id }) => id === original.id).transcript, original.transcript);
    assert.equal((await api.askMeeting(original.id, 'Explain this')).citations[0].sessionId, original.id);
    await assert.rejects(api.askMeeting(first.id, 'Draft should match native eligibility'));
    assert.equal((await api.askMeetings('Planning', 'Explain this')).citations[0].sessionId, 'simulation-harbor-planning');
    await assert.rejects(api.askMeetings('Missing folder', 'Explain this'));
    const refreshed = await api.refreshInsights(original.id);
    assert.ok(refreshed.previousNotes);
    assert.match(refreshed.notes, /SIMULATION/);
    await assert.rejects(api.restoreNotes(original.id, 'stale notes'));
    const restored = await api.restoreNotes(original.id, refreshed.notes);
    assert.equal(restored.notes, refreshed.previousNotes);
    assert.equal(restored.previousNotes, refreshed.notes);
    await api.deleteTranscript(original.id);
    library = await api.bootstrap();
    assert.equal(library.sessions.find(({ id }) => id === first.id).notes, 'First private note');
    await api.deleteSession(first.id);
    library = await api.bootstrap();
    assert.equal(library.sessions.some(({ id }) => id === first.id), false);
    assert.equal(library.sessions.find(({ id }) => id === second.id).notes, 'Second private note');
    await assert.rejects(api.askMeeting(first.id, 'Deleted source must not resolve'));
    await assert.rejects(api.saveSession({ ...first, notes: 'Do not resurrect' }));
  } finally { delete globalThis.window; }
});

test('synthetic questions can fail once, retry, and finish slowly without pretending to use OpenAI', async () => {
  globalThis.window = { location: { search: '?key&askError=once&askDelay=0&meetings=2' } };
  try {
    const source = (await api.bootstrap()).sessions.find(({ id }) => id === 'simulation-harbor-planning');
    await assert.rejects(api.askMeeting(source.id, 'Synthetic retry'), /SIMULATION/);
    const retried = await api.askMeeting(source.id, 'Synthetic retry');
    assert.match(retried.answer, /SIMULATION/);
    window.location.search = '?key&askDelay=20&answer=long';
    let finished = false;
    const pending = api.askMeeting(source.id, 'Synthetic slow answer').then((answer) => { finished = true; return answer; });
    await Promise.resolve();
    assert.equal(finished, false);
    const long = await pending;
    assert.ok(long.answer.length > 6000);
    assert.equal(long.citations[0].sessionId, source.id);
  } finally { delete globalThis.window; }
});

test('restoring the prior notes version sends the expected current document to native core', async () => {
  globalThis.window = {};
  mockIPC((command, payload) => {
    assert.equal(command, 'restore_notes');
    assert.deepEqual(payload, { id: 'one', expectedNotes: 'Current saved document' });
    return { id: 'one', notes: 'Previous saved document', previousNotes: 'Current saved document' };
  });
  try {
    assert.equal((await api.restoreNotes('one', 'Current saved document')).notes, 'Previous saved document');
  } finally { delete globalThis.window; }
});
