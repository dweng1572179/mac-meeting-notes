import { describe, expect, it } from 'vitest';
import { literalHighlights, meetingMarkdown } from './meeting-workspace';
import type { Session } from './types';

const meeting: Session = {
  id: 'meeting-1',
  title: 'Roadmap </script>',
  startedAt: '2026-09-16T09:30:00Z',
  endedAt: null,
  context: 'Quarterly plan <private>',
  attendees: ['Renée', '李雷'],
  folder: 'Planning & delivery',
  originalNotes: 'Keep **my** notes & <details>.',
  transcript: '[System audio · 00:12] Ship 東京 (v2) [soon].',
  enrichedNotes: '## Decision\nShip 東京.',
  status: 'failed',
  error: { code: 'transcription_failed', message: 'A later chunk failed.' },
  audioPath: null,
  microphoneAudioPath: null,
  warnings: ['Microphone <silent> & retry later.'],
  captureHealth: {
    wallSeconds: 75,
    identityChanged: false,
    warnings: [],
    system: { admittedFrames: 0, writtenFrames: 0, capturedSeconds: 0, sampleRate: 48_000, lastCallbackAgeSeconds: null, writeError: null, status: 'no_frames' },
    microphone: { admittedFrames: 1, writtenFrames: 1, capturedSeconds: 72, sampleRate: 48_000, lastCallbackAgeSeconds: 0, writeError: null, status: 'healthy' }
  }
};

describe('literalHighlights', () => {
  it('finds literal punctuation without treating it as a regular expression', () => {
    expect(literalHighlights('Use [draft]. Then [DRAFT].', '[draft]')).toEqual([
      { text: 'Use ', match: false },
      { text: '[draft]', match: true },
      { text: '. Then ', match: false },
      { text: '[DRAFT]', match: true },
      { text: '.', match: false }
    ]);
  });

  it('preserves Unicode text and matches Unicode queries', () => {
    expect(literalHighlights('東京 → café → 東京', '東京')).toEqual([
      { text: '東京', match: true },
      { text: ' → café → ', match: false },
      { text: '東京', match: true }
    ]);
  });

  it('returns the complete transcript when the query is empty', () => {
    expect(literalHighlights('<script>alert("x")</script>', '')).toEqual([
      { text: '<script>alert("x")</script>', match: false }
    ]);
  });
});

describe('meetingMarkdown', () => {
  it('exports every saved section while neutralizing raw HTML', () => {
    const markdown = meetingMarkdown(meeting);

    expect(markdown).toContain('# Roadmap &lt;/script&gt;');
    expect(markdown).toContain('- Date: 2026-09-16T09:30:00Z');
    expect(markdown).toContain('- Attendees: Renée, 李雷');
    expect(markdown).toContain('- Context: Quarterly plan &lt;private&gt;');
    expect(markdown).toContain('- Folder: Planning &amp; delivery');
    expect(markdown).toContain('- Capture coverage: elapsed 1:15; system 0:00; microphone 1:12');
    expect(markdown).toContain('## Original notes\n\nKeep **my** notes &amp; &lt;details&gt;.');
    expect(markdown).toContain('## Enhanced notes\n\n## Decision\nShip 東京.');
    expect(markdown).toContain('## Transcript\n\n> Transcript may be incomplete.\n\n[System audio · 00:12] Ship 東京 (v2) [soon].');
    expect(markdown).toContain('## Capture warnings\n\n- Microphone &lt;silent&gt; &amp; retry later.');
    expect(markdown).not.toContain('<script>');
  });

  it('keeps explicit empty sections so the export is complete', () => {
    const markdown = meetingMarkdown({
      ...meeting,
      originalNotes: '',
      enrichedNotes: null,
      transcript: null,
      warnings: []
    });

    expect(markdown).toContain('## Original notes\n\n_None saved._');
    expect(markdown).toContain('## Enhanced notes\n\n_None saved._');
    expect(markdown).toContain('## Transcript\n\n_None saved._');
    expect(markdown).toContain('## Capture warnings\n\n_None._');
  });

  it('marks a failed partial transcript as incomplete and includes the processing error', () => {
    const markdown = meetingMarkdown(meeting);

    expect(markdown).toContain('- Status: failed');
    expect(markdown).toContain('- Processing error: transcription_failed — A later chunk failed.');
    expect(markdown).toContain('## Transcript\n\n> Transcript may be incomplete.\n\n[System audio');
  });
});
