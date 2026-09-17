import type { Session } from './types';

export type HighlightSegment = { text: string; match: boolean };

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
    section('Original notes', session.originalNotes),
    section('Enhanced notes', session.enrichedNotes),
    transcriptSection,
    section('Capture warnings', warnings, '_None._')
  ].join('\n\n');
}
