<script lang="ts">
  import { errorMessage } from './recovery';
  import { legacyTranscriptParagraphs, transcriptParagraphs, literalHighlights, transcriptTurns, validTopicAnchors } from './meeting-workspace';
  import type { Session } from './types';

  let { session, onExport }: { session: Session; onExport: () => Promise<string> } = $props();
  let query = $state('');
  let actionStatus = $state('');
  let statusError = $state(false);
  let exporting = $state(false);
  let turns = $derived(transcriptTurns(session));
  let hasSpeakerLabels = $derived(turns.some((turn) => turn.speaker !== null));
  let topics = $derived(validTopicAnchors(session.aiSuggestions?.topics, new Set(turns.map((turn) => turn.id))));
  let legacyParagraphs = $derived(legacyTranscriptParagraphs(session.transcript ?? '', query));
  let matchCount = $derived(
    (turns.length ? turns.flatMap((turn) => turn.id === 'legacy-transcript'
      ? legacyTranscriptParagraphs(turn.text, query).map((paragraph) => legacyBlock(paragraph).text)
      : transcriptParagraphs(turn.text, query))
      : legacyParagraphs.map((paragraph) => legacyBlock(paragraph).text))
      .flatMap((text) => literalHighlights(text, query.trim()))
      .filter((segment) => segment.match).length
  );

  const sourceLabel = (source: 'system' | 'microphone' | null) => source === 'system' ? 'System audio' : source === 'microphone' ? 'Microphone' : 'Saved transcript';
  const legacyBlock = (paragraph: string) => {
    const match = paragraph.match(/^\[([^\]]+)\]\s+(System audio|Microphone):\s*\n([\s\S]+)$/u);
    return match ? { meta: `${match[2]} · ${match[1]}`, text: match[3] } : { meta: '', text: paragraph };
  };
  const time = (seconds: number) => {
    const whole = Math.max(0, Math.floor(seconds));
    const minutes = Math.floor(whole / 60);
    return `${minutes}:${String(whole % 60).padStart(2, '0')}`;
  };

  async function copyTranscript() {
    actionStatus = '';
    statusError = false;
    try {
      await navigator.clipboard.writeText(session.transcript ?? '');
      actionStatus = 'Transcript copied.';
    } catch {
      statusError = true;
      actionStatus = 'Could not copy. Select the raw transcript and copy it manually.';
    }
  }

  async function exportTranscript() {
    exporting = true;
    actionStatus = '';
    statusError = false;
    try {
      actionStatus = `Saved to ${await onExport()}`;
    } catch (caught) {
      statusError = true;
      actionStatus = errorMessage(caught, 'The Markdown copy could not be saved. Try again.');
    } finally {
      exporting = false;
    }
  }
</script>

<section class="transcript-workspace" aria-label="Transcript tools and text">
  <header class="transcript-header">
    <p>{session.status !== 'complete'
      ? 'Available text so far. Processing may be incomplete.'
      : hasSpeakerLabels
        ? 'Speaker labels are local to each recorded source section.'
        : 'This transcript has no verified speaker labels.'}</p>
    <div class="transcript-actions">
      <button type="button" onclick={copyTranscript}>Copy</button>
      <button type="button" disabled={exporting} onclick={exportTranscript}>{exporting ? 'Saving…' : 'Export'}</button>
    </div>
  </header>

  {#if topics.length}
    <nav class="topic-nav" aria-label="Transcript topics">
      {#each topics as topic}<a href={`#${topic.startsAtTurnId}`}>{topic.title}</a>{/each}
    </nav>
  {/if}

  <label class="search-field">
    <span>Search</span>
    <input bind:value={query} type="search" placeholder="Find a name, number, or phrase" />
    {#if query.trim()}<small aria-live="polite">{matchCount ? `${matchCount} ${matchCount === 1 ? 'match' : 'matches'}` : 'No matches'}</small>{/if}
  </label>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="transcript-flow" role="region" aria-label="Readable transcript" tabindex="0" dir="auto">
    {#if turns.length}
      {#each turns as turn}
        {@const topic = topics.find((item) => item.startsAtTurnId === turn.id)}
        {#if topic}<h3 class="topic-heading">{topic.title}</h3>{/if}
        <article class="turn" id={turn.id}>
          {#if turn.source}
            <div class="turn-heading">
              {#if turn.speaker}<strong>Speaker {turn.speaker}</strong>{/if}
              <span title={`${sourceLabel(turn.source)} · ${time(turn.startSeconds)}–${time(turn.endSeconds)}`}><time>{time(turn.startSeconds)}</time> · {sourceLabel(turn.source)}{turn.speaker ? ` · ${turn.sectionLabel}` : ''}</span>
            </div>
          {/if}
          {#if turn.id === 'legacy-transcript'}
            {#each legacyTranscriptParagraphs(turn.text, query) as paragraph}
              {@const block = legacyBlock(paragraph)}
              {#if block.meta}<p class="turn-meta"><strong>{block.meta}</strong></p>{/if}
              <p class="legacy-paragraph">{#each literalHighlights(block.text, query.trim()) as segment}{#if segment.match}<mark>{segment.text}</mark>{:else}{segment.text}{/if}{/each}</p>
            {/each}
          {:else}
            {#each transcriptParagraphs(turn.text, query) as paragraph}
              <p class="legacy-paragraph">{#each literalHighlights(paragraph, query.trim()) as segment}{#if segment.match}<mark>{segment.text}</mark>{:else}{segment.text}{/if}{/each}</p>
            {/each}
          {/if}
        </article>
      {/each}
    {:else}
      <div class="legacy-note">This older transcript has no verified speaker turns.</div>
      {#each legacyParagraphs as paragraph}
        {@const block = legacyBlock(paragraph)}
        {#if block.meta}<p class="turn-meta"><strong>{block.meta}</strong></p>{/if}
        <p class="legacy-paragraph">{#each literalHighlights(block.text, query.trim()) as segment}{#if segment.match}<mark>{segment.text}</mark>{:else}{segment.text}{/if}{/each}</p>
      {/each}
    {/if}
  </div>

  <details class="raw-transcript">
    <summary>View original text</summary>
    <pre dir="auto">{session.transcript}</pre>
  </details>
  <p class:error={statusError} class="action-status" aria-live="polite">{actionStatus}</p>
</section>

<style>
  .transcript-workspace { display: grid; gap: 14px; }
  .transcript-header { display: flex; align-items: flex-end; justify-content: space-between; gap: 18px; }
  .transcript-header p, .action-status { margin: 0; color: var(--muted-text); font-size: 12px; line-height: 1.45; }
  .transcript-actions { display: flex; gap: 7px; }
  button { min-height: 34px; padding: 0 11px; border: 1px solid var(--line); border-radius: 8px; color: var(--ink); background: var(--paper); font-size: 12px; font-weight: 500; cursor: pointer; }
  button:hover { background: var(--sidebar); }
  button:disabled { cursor: wait; opacity: .55; }
  .topic-nav { display: flex; flex-wrap: wrap; gap: 6px; }
  .topic-nav a { padding: 5px 9px; border-radius: 999px; color: var(--accent-text); background: var(--accent-soft); font-size: 11px; font-weight: 650; text-decoration: none; }
  .search-field { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 10px; color: var(--muted-text); font-size: 11px; font-weight: 650; }
  .search-field input { min-height: 36px; padding: 0 10px; border: 1px solid var(--line); border-radius: 8px; color: var(--ink); background: color-mix(in srgb, var(--paper) 70%, transparent); font: inherit; font-size: 14px; font-weight: 400; }
  .search-field small { font-size: 11px; font-weight: 400; white-space: nowrap; }
  .transcript-flow { max-height: 58vh; overflow: auto; padding: 4px 16px 4px 0; scroll-behavior: smooth; }
  .turn { scroll-margin-top: 12px; }
  .turn + .turn { margin-top: 22px; }
  .topic-heading { margin: 28px 0 12px; color: var(--ink); font-family: Georgia, ui-serif, serif; font-size: 18px; font-weight: 500; }
  .topic-heading:first-child { margin-top: 0; }
  .turn-heading { display: flex; align-items: baseline; gap: 10px; margin: 0 0 7px; }
  .turn-heading strong { color: var(--ink); font-size: 12px; font-weight: 600; }
  .turn-heading span { color: var(--muted-text); font-size: 11px; font-variant-numeric: tabular-nums; }
  .turn-meta { display: flex; gap: 10px; margin: 0 0 7px; color: var(--muted-text); font-size: 11px; line-height: 1.4; }
  .turn-meta strong { color: var(--accent-text); font-weight: 700; }
  .legacy-paragraph { max-width: 70ch; margin: 0; color: #3e3e36; font-size: 15px; line-height: 1.8; overflow-wrap: anywhere; white-space: pre-wrap; }
  .legacy-paragraph + .legacy-paragraph { margin-top: 16px; }
  .legacy-note { margin-bottom: 16px; color: var(--muted-text); font-size: 11px; }
  mark { border-radius: 3px; color: inherit; background: #e5dda7; }
  .raw-transcript { color: var(--muted-text); font-size: 11px; }
  .raw-transcript summary { width: fit-content; cursor: pointer; font-weight: 650; }
  .raw-transcript pre { max-height: 240px; overflow: auto; margin: 9px 0 0; padding: 12px; border: 1px solid var(--line); border-radius: 10px; color: #4b4a43; background: var(--sidebar); font-family: inherit; font-size: 12px; line-height: 1.55; white-space: pre-wrap; }
  .action-status { min-height: 17px; }
  .action-status.error { color: var(--danger); }
  @media (prefers-reduced-motion: reduce) { .transcript-flow { scroll-behavior: auto; } }
  @media (max-width: 680px) { .transcript-header { align-items: flex-start; flex-direction: column; } .search-field { grid-template-columns: 1fr; gap: 5px; } }
</style>
