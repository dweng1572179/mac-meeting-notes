import { describe, expect, it } from 'vitest';
import { metadataAfterSuggestion, notesAfterSessionChange, unsavedNotesInput } from './MeetingEditor.svelte';
import { captureErrorMessage, elapsedRecordingSeconds } from './RecordingDock.svelte';

it('uses generated notes for the next edit when the open editor has no unsaved draft', () => {
  const notes = notesAfterSessionChange('Earlier draft', 'Generated decision: wait.', false);
  const input = { id: 'one', title: '', context: '', attendees: [], folder: '', originalNotes: '', notes: `${notes}\nConfirm Friday.` };
  expect(unsavedNotesInput(input, 2, 1).notes).toBe('Generated decision: wait.\nConfirm Friday.');
});

it('preserves an unsaved draft, including an intentional blank, when server notes change', () => {
  expect(notesAfterSessionChange('My unsaved correction', 'Generated notes', true)).toBe('My unsaved correction');
  expect(notesAfterSessionChange('', 'Generated notes', true)).toBe('');
});

it('merges a delayed suggestion response without replacing unrelated local metadata', () => {
  const current = { title: 'Typed title', context: 'New local context', folder: 'Local folder', attendees: 'Ari' };
  const staleResponse = { title: 'Suggested title', context: 'Old context', folder: 'Old folder', attendees: 'Old attendee' };

  expect(metadataAfterSuggestion(current, 'title', staleResponse)).toEqual({
    ...current,
    title: 'Suggested title'
  });
});

describe('elapsedRecordingSeconds', () => {
  it('uses monotonic elapsed time without returning a negative value', () => {
    expect(elapsedRecordingSeconds(1_000, 5_250)).toBe(4);
    expect(elapsedRecordingSeconds(5_000, 4_000)).toBe(0);
  });

  it('keeps sleep time reported by the native capture clock in the dock timer', () => {
    expect(elapsedRecordingSeconds(0, 60_000, 120)).toBe(120);
    expect(elapsedRecordingSeconds(0, 125_000, 124)).toBe(125);
  });
});

describe('captureErrorMessage', () => {
  it('points microphone failures to the microphone privacy setting', () => {
    expect(captureErrorMessage({ code: 'microphone_permission', message: 'Permission denied' }, undefined, 'mac')).toContain(
      'Privacy & Security → Microphone'
    );
  });

  it('does not mislabel a microphone codec failure as a privacy failure', () => {
    expect(
      captureErrorMessage({
        code: 'microphone_capture',
        message: 'AudioConverterSetProperty failed with OSStatus 560226676'
      })
    ).not.toContain('Privacy & Security');
  });

  it('points system-audio permission failures to the system-audio privacy setting', () => {
    expect(captureErrorMessage({ code: 'audio_permission', message: 'Permission denied' }, undefined, 'mac')).toContain(
      'Screen & System Audio Recording'
    );
  });

  it('does not mislabel an unsupported audio format as a privacy failure', () => {
    expect(
      captureErrorMessage({
        code: 'audio_capture',
        message: 'AudioConverterSetProperty failed with OSStatus 560226676'
      })
    ).not.toContain('Privacy & Security');
  });
});

describe('captureCoverage', () => {
  it('shows captured source durations separately from elapsed wall time', async () => {
    const { captureCoverage } = await import('./RecordingDock.svelte');
    expect(captureCoverage({ wallSeconds: 8734, system: { capturedSeconds: 0 }, microphone: { capturedSeconds: 4320 } })).toBe(
      'Elapsed 2:25:34 · Captured: system 0:00, microphone 1:12:00'
    );
  });
});

it('does not replay acknowledged notes when queued metadata saves after generation', async () => {
  const { unsavedNotesInput } = await import('./MeetingEditor.svelte');
  const { createAutosave } = await import('./autosave');
  let savedRevision = 0;
  let document = 'Initial';
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => { release = resolve; });
  const saved: import('./types').UpdateSessionInput[] = [];
  const autosave = createAutosave<{ input: import('./types').UpdateSessionInput; revision: number }>(450, async ({ input, revision }) => {
    const value = unsavedNotesInput(input, revision, savedRevision);
    saved.push(value);
    if (saved.length === 1) await blocked;
    if (value.notes !== undefined) { document = value.notes; savedRevision = revision; }
    if (saved.length === 1) document = 'Generated from latest notes';
  });
  const input = { id: 'one', title: 'Before', context: '', attendees: [], folder: '', originalNotes: '', notes: 'Typed notes' };
  autosave.schedule({ input, revision: 1 });
  const flushed = autosave.flush();
  await Promise.resolve();
  autosave.schedule({ input: { ...input, title: 'After' }, revision: 1 });
  release();
  await flushed;
  expect(document).toBe('Generated from latest notes');
  expect(saved[1].title).toBe('After');
  expect(saved[1].notes).toBeUndefined();
  expect(unsavedNotesInput({ ...input, notes: '' }, 2, savedRevision).notes).toBe('');
});
