import { describe, expect, it } from 'vitest';
import { dirtyAiNotesDraft, flushMeetingDrafts, metadataAfterSuggestion, nextMeetingView } from './MeetingEditor.svelte';
import { captureErrorMessage, elapsedRecordingSeconds } from './RecordingDock.svelte';

describe('nextMeetingView', () => {
  it('preserves Original when it was used during processing', () => {
    expect(nextMeetingView('original', 'processing', 'complete', true)).toBe('original');
  });

  it('reveals Enhanced after untouched processing completes', () => {
    expect(nextMeetingView('original', 'processing', 'complete', false)).toBe('enhanced');
  });

  it('keeps the transcript visible when processing finishes', () => {
    expect(nextMeetingView('transcript', 'processing', 'complete', false)).toBe('transcript');
  });
});

describe('dirtyAiNotesDraft', () => {
  it('returns edited text, including an intentional blank, while the editor is open', () => {
    expect(dirtyAiNotesDraft(true, 'Revised notes', 'Generated notes')).toBe('Revised notes');
    expect(dirtyAiNotesDraft(true, '', 'Generated notes')).toBe('');
  });

  it('does not persist a closed or unchanged draft during navigation', () => {
    expect(dirtyAiNotesDraft(false, 'Revised notes', 'Generated notes')).toBeUndefined();
    expect(dirtyAiNotesDraft(true, 'Generated notes', 'Generated notes')).toBeUndefined();
  });
});

describe('flushMeetingDrafts', () => {
  it('waits for original notes before saving a dirty AI draft', async () => {
    const events: string[] = [];
    await flushMeetingDrafts(
      async () => void events.push('original'),
      () => 'AI draft',
      async (draft) => void events.push(draft)
    );
    expect(events).toEqual(['original', 'AI draft']);
  });

  it('includes typing that occurs while the original notes are saving', async () => {
    let draft = 'Earlier draft';
    let saved = '';
    await flushMeetingDrafts(async () => { await Promise.resolve(); draft = 'Latest typed draft'; },
      () => draft, async (value) => { saved = value; });
    expect(saved).toBe('Latest typed draft');
  });

  it('rejects when the AI draft cannot be saved so navigation stays blocked', async () => {
    await expect(
      flushMeetingDrafts(async () => {}, () => '', async () => { throw new Error('disk full'); })
    ).rejects.toThrow('disk full');
  });
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
    expect(captureErrorMessage({ code: 'microphone_permission', message: 'Permission denied' })).toContain(
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
    expect(captureErrorMessage({ code: 'audio_permission', message: 'Permission denied' })).toContain(
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
