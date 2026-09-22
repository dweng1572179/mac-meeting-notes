// With `npm run dev` running:
// agent-browser eval "import('/scripts/check-editor.js').then(m => m.checkEditor())"
import { mount, tick, unmount } from 'svelte';
import { mockIPC, mockWindows, clearMocks } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import App from '../src/App.svelte';

export async function checkEditor() {
  let session = {
    id: 'synthetic-editor', title: '[SIMULATION] Editor synchronization', startedAt: '2026-01-01T10:00:00Z',
    endedAt: '2026-01-01T10:05:00Z', context: '', attendees: [], folder: '', originalNotes: '',
    notes: 'Earlier document', enrichedNotes: null, transcript: '[SIMULATION] Final decision: wait.',
    transcription: [], status: 'processing', error: null, audioPath: null, microphoneAudioPath: null
  };
  const saved = [];
  mockIPC((command, payload) => {
    if (command === 'bootstrap') return { sessions: [session], hasApiKey: true };
    if (command === 'save_session') {
      saved.push(payload.input);
      session = { ...session, ...payload.input };
      return session;
    }
    if (command === 'refresh_insights') return { ...session, status: 'processing' };
    throw new Error(`Unexpected native command: ${command}`);
  }, { shouldMockEvents: true });
  mockWindows('main');
  const target = document.createElement('section');
  document.body.prepend(target);
  const app = mount(App, { target });
  const checks = [];
  const check = (name, passed) => { checks.push({ name, passed }); if (!passed) throw new Error(name); };
  const settle = async () => { await new Promise(resolve => setTimeout(resolve, 0)); await tick(); };
  try {
    await settle();
    target.querySelector('.recent button').click();
    await settle();
    const editor = target.querySelector('#meeting-notes');
    editor.focus();
    session = { ...session, status: 'complete', notes: 'Generated decision: wait.', enrichedNotes: 'Generated decision: wait.' };
    await emit('session-updated', session);
    await settle();
    check('focused clean editor adopts completed generation', editor.value === session.notes && document.activeElement === editor);
    editor.value += '\nConfirm Friday.';
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    await emit('session-updated', { ...session, notes: 'A later server event' });
    await settle();
    check('incoming generation preserves dirty draft', editor.value === 'Generated decision: wait.\nConfirm Friday.');
    // Flush through the real editor action instead of waiting for its debounce timer.
    Array.from(target.querySelectorAll('button')).find(button => button.textContent.trim() === 'Done').click();
    await settle();
    check('next save includes generated notes and the new edit', saved.at(-1)?.notes === 'Generated decision: wait.\nConfirm Friday.');
    target.querySelector('.meeting-menu-trigger').click();
    await settle();
    Array.from(target.querySelectorAll('.meeting-actions button')).find(button => button.textContent.includes('Update notes')).click();
    await settle();
    check('refresh reports start, not premature success', target.textContent.includes('Started updating notes from saved text.') && !target.textContent.includes('Notes updated from saved text.'));
  } finally {
    await unmount(app);
    target.remove();
    clearMocks();
  }
  return checks;
}
