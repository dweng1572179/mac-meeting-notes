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
