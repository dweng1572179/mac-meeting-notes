import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import TranscriptView from './TranscriptView.svelte';
import InlineMarkdown from './InlineMarkdown.svelte';
import type { Session } from './types';

describe('transcript presentation', () => {
  it('shows plain recording chunks once in the reading flow without repeated section headings', () => {
    const session = {
      status: 'complete', transcript: 'Opening words. Closing words.',
      transcription: [{ source: 'system', chunks: [
        { startSeconds: 0, durationSeconds: 60, transcript: 'Opening words.' },
        { startSeconds: 60, durationSeconds: 60, transcript: 'Closing words.' }
      ] }]
    } as Session;
    const html = render(TranscriptView, { props: { session, onExport: async () => '' } }).body;
    const reading = html.split('aria-label="Readable transcript"')[1].split('<details')[0];
    expect(reading).not.toContain('Transcript section');
    expect(reading.match(/Opening words\./g)).toHaveLength(1);
    expect(reading.match(/Closing words\./g)).toHaveLength(1);
    expect(reading).not.toContain('Speaker');
  });

  it('renders emphasis while treating model-provided HTML as text', () => {
    const html = render(InlineMarkdown, { props: { text: '**Decision** <img src=x onerror=alert(1)>' } }).body;
    expect(html).toMatch(/<strong[^>]*>Decision<\/strong>/u);
    expect(html).not.toContain('**Decision**');
    expect(html).toContain('&lt;img');
    expect(html).not.toContain('<img');
  });
});
