import { describe, expect, it } from 'vitest';
import { nextMeetingView } from './MeetingEditor.svelte';
import { captureErrorMessage, elapsedRecordingSeconds } from './RecordingDock.svelte';

describe('nextMeetingView', () => {
  it('preserves Original when it was used during processing', () => {
    expect(nextMeetingView('original', 'processing', 'complete', true)).toBe('original');
  });

  it('reveals Enhanced after untouched processing completes', () => {
    expect(nextMeetingView('original', 'processing', 'complete', false)).toBe('enhanced');
  });
});

describe('elapsedRecordingSeconds', () => {
  it('uses monotonic elapsed time without returning a negative value', () => {
    expect(elapsedRecordingSeconds(1_000, 5_250)).toBe(4);
    expect(elapsedRecordingSeconds(5_000, 4_000)).toBe(0);
  });
});

describe('captureErrorMessage', () => {
  it('points microphone failures to the microphone privacy setting', () => {
    expect(captureErrorMessage({ code: 'microphone_capture', message: 'Permission denied' })).toContain(
      'Privacy & Security → Microphone'
    );
  });

  it('points system capture failures to the system-audio privacy setting', () => {
    expect(captureErrorMessage({ code: 'audio_capture', message: 'Permission denied' })).toContain(
      'Screen & System Audio Recording'
    );
  });
});
