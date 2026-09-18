import type { Session } from './types';

export type HighlightSegment = { text: string; match: boolean };
export type TranscriptTurn = {
  id: string;
  speakerKey: string | null;
  speaker: string | null;
  sectionLabel: string;
  source: 'system' | 'microphone' | null;
  startSeconds: number;
  endSeconds: number;
  text: string;
};

type TranscriptTrack = {
  source: 'system' | 'microphone';
  chunks: {
    startSeconds: number;
    durationSeconds?: number;
    transcript?: string | null;
    segmentIndex?: number | null;
    segments?: { id: string; speaker: string; startSeconds: number; endSeconds: number; text: string }[];
  }[];
};

export function meetingNotes(session: Pick<Session, 'notes' | 'originalNotes' | 'enrichedNotes' | 'editedEnrichedNotes'>): string {
  if (session.notes !== null && session.notes !== undefined) return session.notes;
  const summary = session.editedEnrichedNotes ?? session.enrichedNotes ?? '';
  const original = session.originalNotes.trim();
  if (!summary.trim()) return session.originalNotes;
  return original && !summary.includes(original) ? `${summary}\n\n${session.originalNotes}` : summary;
}

export function transcriptTurns(input: TranscriptTrack[] | { transcription?: TranscriptTrack[]; transcript?: string | null } = []): TranscriptTurn[] {
  const tracks = Array.isArray(input) ? input : input.transcription ?? [];
  const turns: TranscriptTurn[] = tracks.flatMap((track): TranscriptTurn[] => track.chunks.flatMap((chunk): TranscriptTurn[] => {
    const chunkKey = chunk.segmentIndex === null || chunk.segmentIndex === undefined
      ? `${track.source}:offset-${Math.round(chunk.startSeconds * 1000)}`
      : `${track.source}:segment-${chunk.segmentIndex}:offset-${Math.round(chunk.startSeconds * 1000)}`;
    const segments = chunk.segments ?? [];
    if (!segments.length) {
      return chunk.transcript?.trim() ? [{
        id: `${chunkKey}:text`,
        speakerKey: null,
        speaker: null,
        sectionLabel: chunk.segmentIndex === null || chunk.segmentIndex === undefined ? 'Saved section' : `Part ${chunk.segmentIndex + 1}`,
        source: track.source,
        startSeconds: chunk.startSeconds,
        endSeconds: chunk.startSeconds + (chunk.durationSeconds ?? 0),
        text: chunk.transcript
      }] : [];
    }
    return segments.map((segment) => ({
      id: `${chunkKey}:${segment.id}`,
      speakerKey: `${chunkKey}:${segment.speaker}`,
      speaker: segment.speaker,
      sectionLabel: chunk.segmentIndex === null || chunk.segmentIndex === undefined ? 'Saved section' : `Part ${chunk.segmentIndex + 1}`,
      source: track.source,
      startSeconds: chunk.startSeconds + segment.startSeconds,
      endSeconds: chunk.startSeconds + segment.endSeconds,
      text: segment.text
    }));
  })).sort((left, right) => left.startSeconds - right.startSeconds || left.id.localeCompare(right.id));
  if (!turns.length && !Array.isArray(input) && input.transcript?.trim()) {
    return [{ id: 'legacy-transcript', speakerKey: null, speaker: null, sectionLabel: 'Legacy recording', source: null, startSeconds: 0, endSeconds: 0, text: input.transcript }];
  }
  return turns;
}

export function validTopicAnchors<T extends { startsAtTurnId: string }>(topics: T[] = [], turnIds: Set<string>) {
  return topics.filter((topic) => turnIds.has(topic.startsAtTurnId));
}

export function legacyTranscriptParagraphs(transcript: string, query = '') {
  const text = transcript.split(/\n\s*\n/u).filter((paragraph) =>
    !paragraph.trim().startsWith('Source tracks overlap in time.')
  ).join('\n\n');
  return transcriptParagraphs(text, query);
}

export function transcriptParagraphs(transcript: string, query = '') {
  return transcript.split(/\n\s*\n/u).map((paragraph) => paragraph.trim()).filter(Boolean).flatMap((paragraph) => {
    // Keep phrases contiguous during search, including across display-only breaks.
    if (query.trim() || paragraph.length <= 600) return [paragraph];
    const result: string[] = [];
    let current = '';
    for (const { segment } of new Intl.Segmenter(undefined, { granularity: 'sentence' }).segment(paragraph)) {
      current += segment;
      if (current.length >= 420) { result.push(current.trim()); current = ''; }
    }
    if (current.trim()) result.push(current.trim());
    return result;
  });
}

export function literalHighlights(text: string, query: string): HighlightSegment[] {
  if (!query) return [{ text, match: false }];
  const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu');
  const segments: HighlightSegment[] = [];
  let cursor = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index;
    if (index > cursor) segments.push({ text: text.slice(cursor, index), match: false });
    segments.push({ text: match[0], match: true });
    cursor = index + match[0].length;
  }
  if (cursor < text.length) segments.push({ text: text.slice(cursor), match: false });
  return segments.length ? segments : [{ text, match: false }];
}

export function meetingMarkdown(session: Session): string {
  const safe = (value: string) => value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const duration = (value: number) => {
    const seconds = Math.max(0, Math.floor(value));
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  };
  const section = (heading: string, value: string | null, empty = '_None saved._') =>
    `## ${heading}\n\n${value ? safe(value) : empty}`;
  const warnings = session.warnings?.length
    ? session.warnings.map((warning) => `- ${warning}`).join('\n')
    : '_None._';
  const metadata = [
    `- Date: ${safe(session.startedAt)}`,
    `- Status: ${safe(session.status)}`,
    `- Attendees: ${session.attendees.length ? safe(session.attendees.join(', ')) : 'None'}`,
    `- Context: ${session.context ? safe(session.context) : 'None'}`,
    `- Folder: ${session.folder ? safe(session.folder) : 'None'}`
  ];
  if (session.captureHealth) {
    metadata.push(
      `- Capture coverage: elapsed ${duration(session.captureHealth.wallSeconds)}; system ${duration(session.captureHealth.system.capturedSeconds)}; microphone ${duration(session.captureHealth.microphone.capturedSeconds)}`
    );
  }
  if (session.error) {
    metadata.push(`- Processing error: ${safe(session.error.code)} — ${safe(session.error.message)}`);
  }
  const transcriptSection = session.transcript
    ? `## Transcript\n\n${session.status !== 'complete' ? '> Transcript may be incomplete.\n\n' : ''}${safe(session.transcript)}`
    : '## Transcript\n\n_None saved._';

  return [
    `# ${safe(session.title || 'Untitled meeting')}`,
    metadata.join('\n'),
    section('Notes', meetingNotes(session)),
    transcriptSection,
    section('Capture warnings', warnings, '_None._')
  ].join('\n\n');
}
