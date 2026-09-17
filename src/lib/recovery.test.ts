import { describe, expect, it } from 'vitest';
import { errorMessage, processingLabel, recordingTranscriptionLabel, recoveryActions } from './recovery';
import type { Session } from './types';

const failed = (overrides: Partial<Session> = {}) => ({
  status: 'failed', error: { code: 'network', message: 'Connection failed. Request ID: req_123' },
  audioPath: '/saved.m4a', microphoneAudioPath: null, transcript: null,
  ...overrides
} as Session);

it('distinguishes recording from pending, caught-up, and paused transcription', () => {
  const recording = failed({ status: 'recording', segmentedCapture: true, error: null });
  expect(recordingTranscriptionLabel(recording)).toBe('Transcript starts after the first minute');
  recording.transcription = [{ source: 'system', chunks: [{ startSeconds: 0, durationSeconds: 60, transcript: null }] }];
  expect(recordingTranscriptionLabel(recording)).toBe('Transcribing in the background');
  recording.transcription[0].chunks[0].transcript = 'Saved words';
  expect(recordingTranscriptionLabel(recording)).toBe('Transcript caught up');
  recording.liveTranscriptionError = { code: 'network', message: 'Offline' };
  expect(recordingTranscriptionLabel(recording)).toBe('Transcript paused · audio kept');
});

describe('error recovery', () => {
  it('keeps structured backend details and adds only relevant permission guidance', () => {
    expect(errorMessage({ code: 'rate_limit', message: 'Try later. Request ID: req_123' })).toBe('Try later. Request ID: req_123');
    expect(errorMessage({ code: 'microphone_permission', message: 'Microphone denied.' })).toContain('Privacy & Security → Microphone');
    expect(errorMessage(new Error('Connection lost'))).toBe('Connection lost');
  });

  it('offers settings for key errors and resumes saved transcription after network errors', () => {
    expect(recoveryActions(failed({ error: { code: 'invalid_api_key', message: 'Invalid key.' } })).settings).toBe(true);
    expect(recoveryActions(failed()).retryLabel).toBe('Resume processing');
    expect(recoveryActions(failed({ transcript: 'Saved words', audioPath: null })).retryLabel).toBe('Retry notes');
  });

  it('explains silence, permits explicit retranscription, and avoids retrying zero frames', () => {
    const silent = failed({ error: { code: 'no_speech', message: 'No speech was detected.' }, transcription: [{ source: 'microphone', chunks: [{ startSeconds: 0, durationSeconds: 15, transcript: '' }] }] });
    expect(recoveryActions(silent).newMeeting).toBe(true);
    expect(recoveryActions(silent).retryLabel).toBe('Retry transcription');
    expect(recoveryActions({ ...silent, transcription: [{ source: 'system', chunks: [] }] }).retryLabel).toBeNull();
    expect(recoveryActions(failed({ audioPath: null })).retryLabel).toBeNull();
  });

  it('distinguishes writing notes from transcription progress', () => {
    expect(processingLabel(failed({ transcript: 'Saved words' }))).toBe('Writing AI notes…');
    expect(processingLabel(failed({ transcription: [{ source: 'system', chunks: [
      { startSeconds: 0, durationSeconds: 300, transcript: 'Saved' },
      { startSeconds: 300, durationSeconds: 20, transcript: null }
    ] }] }))).toBe('Transcribing · 1 of 2 parts saved');
  });
});
