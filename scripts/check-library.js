// With `npm run dev` running, open the preview and run:
// agent-browser eval "import('/scripts/check-library.js').then(m => m.checkLibrary())"
import { mount, tick, unmount } from 'svelte';
import { mockIPC, mockWindows, clearMocks } from '@tauri-apps/api/mocks';
import App from '../src/App.svelte';

export async function checkLibrary() {
  const base = {
    endedAt: null, context: '', attendees: [], originalNotes: 'keep these notes',
    transcript: null, enrichedNotes: null, status: 'draft', error: null, audioPath: null
  };
  const sessions = [
    { ...base, id: 'newer', title: 'Visible meeting', startedAt: '2026-01-01T10:00:00.1Z' },
    { ...base, id: 'older', title: 'Hidden meeting', startedAt: '2026-01-01T10:00:00Z' }
  ];
  mockIPC((command) => {
    if (command === 'bootstrap') return { sessions, hasApiKey: true };
    if (command === 'delete_session' || command.startsWith('plugin:event|')) return;
    throw new Error(`Unexpected native command: ${command}`);
  });
  mockWindows('main');
  const target = document.createElement('section');
  document.body.prepend(target);
  const app = mount(App, { target });
  const checks = [];
  const check = (name, passed) => checks.push({ name, passed });
  const settle = async () => { await new Promise(resolve => setTimeout(resolve, 0)); await tick(); };
  try {
    await settle();
    check('mixed fractional precision keeps backend recency order',
      target.querySelector('.recent button span')?.textContent === 'Visible meeting');
    check('selected session is programmatically identified',
      target.querySelector('.recent button[aria-current="page"] span')?.textContent === 'Visible meeting');
    target.querySelector('.nav-item').click();
    await settle();
    check('Home is programmatically identified when selected',
      target.querySelector('.nav-item')?.getAttribute('aria-current') === 'page');
    Array.from(target.querySelectorAll('.recent button')).find(button => button.textContent.includes('Visible meeting')).click();
    await settle();
    const search = target.querySelector('input[type="search"]');
    search.value = 'Visible meeting';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    check('search filters the sidebar', target.querySelectorAll('.recent button').length === 1);
    check('action popover retains native button semantics',
      !target.querySelector('[role="menu"], [role="menuitem"]'));
    target.querySelector('.meeting-menu-trigger').click();
    const deletion = Array.from(target.querySelectorAll('.meeting-actions button')).find(button => button.textContent === 'Delete meeting');
    deletion.click();
    await settle();
    target.querySelector('.delete-button').click();
    await settle();
    check('deleting a searched meeting never selects a hidden meeting',
      !target.querySelector('.meeting-document'));
  } finally {
    await unmount(app);
    target.remove();
    clearMocks();
  }
  if (checks.some(check => !check.passed)) throw new Error(JSON.stringify(checks));
  return checks;
}
