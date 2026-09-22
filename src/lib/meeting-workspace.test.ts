import { describe, expect, it } from 'vitest';
import {
  meetingNotes,
  literalHighlights,
  meetingMarkdown,
  legacyTranscriptParagraphs,
  transcriptParagraphs,
  transcriptTurns,
  validTopicAnchors
} from './meeting-workspace';
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
    expect(markdown).not.toContain('## Your notes');
    expect(markdown).toContain('## Notes\n\n## Decision\nShip 東京.');
    expect(markdown).toContain('## Transcript\n\n> Transcript may be incomplete.\n\n[System audio · 00:12] Ship 東京 (v2) [soon].');
    expect(markdown).toContain('## Capture warnings\n\n- Microphone &lt;silent&gt; &amp; retry later.');
    expect(markdown).not.toContain('<script>');
  });

  it('exports an intentionally blank AI-note edit instead of the generated baseline', () => {
    const markdown = meetingMarkdown({ ...meeting, notes: '' });

    expect(meetingNotes({ ...meeting, notes: '' })).toBe('');
    expect(markdown).toContain('## Notes\n\n_None saved._');
    expect(markdown).not.toContain('## Decision');
  });

  it('keeps explicit empty sections so the export is complete', () => {
    const markdown = meetingMarkdown({
      ...meeting,
      originalNotes: '',
      enrichedNotes: null,
      transcript: null,
      warnings: []
    });

    expect(markdown).toContain('## Notes\n\n_None saved._');
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

describe('workspace labels and transcript structure', () => {
  it('orders turns chronologically and scopes identical speaker labels to each source chunk', () => {
    const turns = transcriptTurns([
      {
        source: 'microphone',
        chunks: [{ startSeconds: 60, durationSeconds: 30, transcript: 'Later words.', segmentIndex: 2, segments: [
          { id: 'turn-1', speaker: 'A', startSeconds: 2, endSeconds: 5, text: 'Later words.' }
        ] }]
      },
      {
        source: 'system',
        chunks: [{ startSeconds: 0, durationSeconds: 30, transcript: 'Earlier words.', segmentIndex: 0, segments: [
          { id: 'turn-1', speaker: 'A', startSeconds: 1, endSeconds: 4, text: 'Earlier words.' }
        ] }]
      }
    ]);

    expect(turns.map(({ id, speakerKey, text }) => ({ id, speakerKey, text }))).toEqual([
      { id: 'system:segment-0:offset-0:turn-1', speakerKey: 'system:segment-0:offset-0:A', text: 'Earlier words.' },
      { id: 'microphone:segment-2:offset-60000:turn-1', speakerKey: 'microphone:segment-2:offset-60000:A', text: 'Later words.' }
    ]);
  });

  it('keeps plain chunks alongside diarized turns in mixed recordings', () => {
    const turns = transcriptTurns({
      transcript: 'Complete saved transcript.',
      transcription: [{ source: 'microphone', chunks: [
        { startSeconds: 0, durationSeconds: 10, transcript: 'Plain opening.', segmentIndex: 0, segments: [] },
        { startSeconds: 10, durationSeconds: 10, transcript: 'Spoken turn.', segmentIndex: 1, segments: [
          { id: 'speaker-turn', speaker: 'B', startSeconds: 0, endSeconds: 3, text: 'Spoken turn.' }
        ] }
      ] }]
    });

    expect(turns.map(({ id, speakerKey, text }) => ({ id, speakerKey, text }))).toEqual([
      { id: 'microphone:segment-0:offset-0:text', speakerKey: null, text: 'Plain opening.' },
      { id: 'microphone:segment-1:offset-10000:speaker-turn', speakerKey: 'microphone:segment-1:offset-10000:B', text: 'Spoken turn.' }
    ]);
  });

  it('keeps canonical words searchable without speaker attribution when segment text disagrees', () => {
    const canonical = 'We discussed the budget. Final decision: do not sign.';
    const turns = transcriptTurns([{ source: 'system', chunks: [{
      startSeconds: 60, durationSeconds: 30, segmentIndex: 1, transcript: canonical,
      segments: [{ id: 's0', speaker: 'A', startSeconds: 1, endSeconds: 5, text: 'We discussed the budget. Sign.' }]
    }] }]);

    expect(turns).toEqual([{
      id: 'system:segment-1:offset-60000:text', speakerKey: null, speaker: null,
      sectionLabel: 'Part 2', source: 'system', startSeconds: 60, endSeconds: 90,
      text: canonical
    }]);
    expect(turns.flatMap((turn) => literalHighlights(turn.text, 'do not sign')).filter((part) => part.match))
      .toEqual([{ text: 'do not sign', match: true }]);
  });

  it('retains speaker turns when only whitespace differs from canonical words', () => {
    const turns = transcriptTurns([{ source: 'microphone', chunks: [{
      startSeconds: 0, durationSeconds: 10, transcript: '  First\u0085words.\tLater\nwords.  ',
      segments: [
        { id: 's0', speaker: 'A', startSeconds: 0, endSeconds: 2, text: 'First words.' },
        { id: 's1', speaker: 'B', startSeconds: 3, endSeconds: 5, text: 'Later words.' }
      ]
    }] }]);

    expect(turns.map(({ speaker, text }) => ({ speaker, text }))).toEqual([
      { speaker: 'A', text: 'First words.' }, { speaker: 'B', text: 'Later words.' }
    ]);
  });

  it('gives a legacy whole transcript the backend topic-anchor ID', () => {
    expect(transcriptTurns({ transcript: 'Legacy saved words.', transcription: [] })).toEqual([
      expect.objectContaining({ id: 'legacy-transcript', speakerKey: null, text: 'Legacy saved words.' })
    ]);
  });

  it('shows only topic anchors that point to a visible turn', () => {
    expect(validTopicAnchors([
      { title: 'Budget', startsAtTurnId: 'system:segment-0:offset-0:turn-1', evidence: [] },
      { title: 'Invented', startsAtTurnId: 'missing', evidence: [] }
    ], new Set(['system:segment-0:offset-0:turn-1']))).toEqual([
      { title: 'Budget', startsAtTurnId: 'system:segment-0:offset-0:turn-1', evidence: [] }
    ]);
  });

  it('turns legacy text into readable paragraphs without inventing speakers', () => {
    expect(legacyTranscriptParagraphs('First thought.\n\nSecond thought.\nStill second.')).toEqual([
      'First thought.',
      'Second thought.\nStill second.'
    ]);
  });

  it('breaks long speech at sentence boundaries without changing or dropping words', () => {
    const text = 'We need to review the budget before making this decision. '.repeat(24).trim();
    const paragraphs = legacyTranscriptParagraphs(text);
    expect(paragraphs.length).toBeGreaterThan(1);
    expect(paragraphs.join(' ')).toBe(text);
    expect(paragraphs.every((paragraph) => paragraph.endsWith('.'))).toBe(true);
  });

  it('keeps structured speech that resembles legacy boilerplate and searches across display breaks', () => {
    expect(transcriptParagraphs('Source tracks overlap in time. We should update the recording pipeline.')).toEqual([
      'Source tracks overlap in time. We should update the recording pipeline.'
    ]);
    const text = 'Review the budget before making this decision. '.repeat(24).trim();
    expect(transcriptParagraphs(text, 'decision. Review')).toEqual([text]);
    expect(legacyTranscriptParagraphs('Source tracks overlap in time.\n\nSaved words.')).toEqual(['Saved words.']);
  });
});

 it('uses one notes document for old recordings, drafts, and intentional blank edits', () => {
  expect(meetingNotes(meeting)).toBe(`${meeting.enrichedNotes}\n\n${meeting.originalNotes}`);
  expect(meetingNotes({ ...meeting, enrichedNotes: null })).toBe(meeting.originalNotes);
  expect(meetingNotes({ ...meeting, enrichedNotes: '  ' })).toBe(meeting.originalNotes);
  expect(meetingNotes({ ...meeting, enrichedNotes: null, notes: 'Draft edit' })).toBe('Draft edit');
  expect(meetingNotes({ ...meeting, notes: '' })).toBe('');
});

it('keeps legacy manual additions visible without duplicate ownership sections', () => {
  const legacy = { ...meeting, originalNotes: 'Call Joe Friday', enrichedNotes: '## Summary\nBudget review' };
  expect(meetingNotes(legacy)).toBe('## Summary\nBudget review\n\nCall Joe Friday');
  expect(meetingMarkdown(legacy)).toContain('Call Joe Friday');
  expect(meetingNotes({ ...legacy, enrichedNotes: 'Call Joe Friday' })).toBe('Call Joe Friday');
  expect(meetingNotes({ ...legacy, editedEnrichedNotes: '' })).toBe('Call Joe Friday');
  expect(meetingNotes({ ...legacy, notes: 'Latest complete document' })).toBe('Latest complete document');
});
