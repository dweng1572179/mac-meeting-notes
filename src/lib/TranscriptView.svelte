<script lang="ts">
  import { literalHighlights } from './meeting-workspace';

  let {
    transcript,
    partial = false,
    onExport
  }: {
    transcript: string;
    partial?: boolean;
    onExport: () => Promise<string>;
  } = $props();

  let query = $state('');
  let copyStatus = $state('');
  let exportStatus = $state('');
  let statusError = $state(false);
  let exporting = $state(false);
  let segments = $derived(literalHighlights(transcript, query.trim()));
  let matchCount = $derived(segments.filter((segment) => segment.match).length);

  async function copyTranscript() {
    copyStatus = '';
    exportStatus = '';
    statusError = false;
    try {
      await navigator.clipboard.writeText(transcript);
      copyStatus = 'Transcript copied.';
    } catch {
      statusError = true;
      copyStatus = 'Could not copy. Select the transcript text and copy it manually.';
    }
  }

  async function exportTranscript() {
    exporting = true;
    exportStatus = '';
    copyStatus = '';
    statusError = false;
    try {
      exportStatus = `Saved to ${await onExport()}`;
    } catch (error) {
      statusError = true;
      exportStatus =
        error && typeof error === 'object' && 'message' in error && typeof error.message === 'string'
          ? error.message
          : 'The Markdown copy could not be saved. Try again.';
    } finally {
      exporting = false;
    }
  }
</script>

<section class="transcript-workspace" aria-labelledby="transcript-title">
  <header>
    <div>
      <h2 id="transcript-title">Transcript</h2>
      <p>{partial ? 'Available transcript. Processing may be incomplete.' : 'Saved transcript. Source and timing labels appear where captured.'}</p>
    </div>
    <div class="transcript-actions">
      <button type="button" onclick={copyTranscript}>Copy full text</button>
      <button type="button" disabled={exporting} onclick={exportTranscript}>
        {exporting ? 'Saving…' : 'Export Markdown'}
      </button>
    </div>
  </header>

  <label class="search-field">
    <span>Search transcript</span>
    <input bind:value={query} type="search" placeholder="Find a name, number, or phrase" />
  </label>
  {#if query.trim()}
    <p class="search-count" aria-live="polite">{matchCount ? `${matchCount} ${matchCount === 1 ? 'match' : 'matches'}` : 'No matches'}</p>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="transcript-text" role="region" aria-label="Transcript text" tabindex="0" dir="auto">
    {#each segments as segment}
      {#if segment.match}<mark>{segment.text}</mark>{:else}{segment.text}{/if}
    {/each}
  </div>
  <p class:error={statusError} class="action-status" aria-live="polite">
    {exportStatus || copyStatus}
  </p>
  <p class="export-hint">Export saves original notes, enhanced notes, transcript, and capture warnings to Downloads.</p>
</section>

<style>
  .transcript-workspace { display: grid; gap: 14px; }
  header { display: flex; align-items: flex-end; justify-content: space-between; gap: 18px; }
  h2 { margin: 0; font-family: Georgia, 'Times New Roman', serif; font-size: 22px; }
  header p, .search-count, .action-status, .export-hint { margin: 5px 0 0; color: var(--muted-text); font-size: 13px; line-height: 1.45; }
  .transcript-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  button { min-height: 36px; padding: 0 12px; border: 1px solid var(--line); border-radius: 8px; color: var(--ink); background: var(--paper); font-weight: 600; cursor: pointer; }
  button:hover { background: var(--sidebar); }
  button:disabled { cursor: wait; opacity: .6; }
  .search-field { display: grid; gap: 6px; color: var(--muted-text); font-size: 12px; font-weight: 700; letter-spacing: .02em; }
  .search-field input { min-height: 40px; padding: 0 12px; border: 1px solid var(--line); border-radius: 8px; color: var(--ink); background: var(--paper); font: inherit; font-size: 16px; font-weight: 400; letter-spacing: 0; }
  .transcript-text { max-height: 48vh; overflow: auto; padding: 18px; border: 1px solid var(--line); border-radius: 12px; background: var(--paper); white-space: pre-wrap; overflow-wrap: anywhere; font-size: 14px; line-height: 1.72; }
  mark { border-radius: 3px; color: inherit; background: #e5dda7; }
  .action-status.error { color: var(--danger); }
  .export-hint { margin-top: -8px; }
  @media (max-width: 680px) { header { align-items: stretch; flex-direction: column; } .transcript-actions { justify-content: flex-start; } }
</style>
