<script lang="ts">
  import type { MeetingAnswer } from './types';
  import { errorMessage } from './recovery';

  let {
    hasApiKey,
    onAsk,
    onOpenSettings
  }: {
    hasApiKey: boolean;
    onAsk: (question: string) => Promise<MeetingAnswer>;
    onOpenSettings: () => void;
  } = $props();

  let question = $state('');
  let answer = $state<MeetingAnswer | null>(null);
  let error = $state('');
  let asking = $state(false);

  async function ask(event: SubmitEvent) {
    event.preventDefault();
    if (asking) return;
    const submitted = question.trim();
    if (!submitted) return;
    if (!hasApiKey) {
      onOpenSettings();
      return;
    }
    asking = true;
    error = '';
    answer = null;
    try {
      answer = await onAsk(submitted);
    } catch (caught) {
      error = errorMessage(caught, 'This meeting could not be read. Your question is still here; try again.');
    } finally {
      asking = false;
    }
  }
</script>

<section class="meeting-question" aria-labelledby="meeting-question-title">
  <div class="question-heading">
    <h2 id="meeting-question-title">Ask this meeting</h2>
    <p>Answers use only this meeting and include exact supporting excerpts.</p>
  </div>
  <form onsubmit={ask}>
    <label for="single-meeting-question">Question</label>
    <textarea id="single-meeting-question" bind:value={question} oninput={() => { answer = null; error = ''; }} rows="3" maxlength="2000" dir="auto" disabled={asking} placeholder="What decision was made, and what evidence supports it?"></textarea>
    <button type="submit" disabled={asking || !question.trim()}>
      {asking ? 'Reading meeting…' : hasApiKey ? 'Ask this meeting' : 'Add OpenAI key'}
    </button>
  </form>

  <div class="answer" aria-live="polite">
    {#if error}
      <p class="error" role="alert">{error}</p>
    {:else if answer}
      <p class="answer-copy" dir="auto">{answer.answer}</p>
      {#if answer.citations.length}
        <ol aria-label="Meeting sources">
          {#each answer.citations as citation}
            <li><blockquote dir="auto">{citation.excerpt}</blockquote></li>
          {/each}
        </ol>
      {/if}
    {/if}
  </div>
</section>

<style>
  .meeting-question { display: grid; grid-template-columns: minmax(0, .72fr) minmax(0, 1fr); gap: 24px; padding-block: 26px; border-top: 1px solid var(--line); }
  .question-heading h2 { margin: 0; font-family: Georgia, 'Times New Roman', serif; font-size: 22px; }
  .question-heading p { max-width: 34ch; margin: 7px 0 0; color: var(--muted-text); font-size: 13px; line-height: 1.5; }
  form { display: grid; gap: 7px; min-width: 0; }
  label { color: var(--muted-text); font-size: 12px; font-weight: 700; }
  textarea { width: 100%; min-height: 84px; resize: vertical; padding: 11px 12px; border: 1px solid var(--line); border-radius: 9px; color: var(--ink); background: var(--paper); font: inherit; font-size: 16px; line-height: 1.5; }
  button { justify-self: end; min-height: 38px; padding: 0 14px; border: 0; border-radius: 8px; color: #fff; background: var(--accent-text); font-weight: 700; cursor: pointer; }
  button:disabled { cursor: not-allowed; opacity: .55; }
  .answer { grid-column: 2; min-width: 0; }
  .answer-copy, .error { margin: 0; line-height: 1.6; overflow-wrap: anywhere; }
  .error { color: var(--danger); }
  ol { display: grid; gap: 8px; margin: 14px 0 0; padding: 0; list-style: none; }
  blockquote { margin: 0; padding: 11px 13px; border: 1px solid var(--line); border-radius: 10px; color: var(--muted-text); background: var(--paper); font-size: 13px; line-height: 1.55; overflow-wrap: anywhere; }
  @media (max-width: 760px) { .meeting-question { grid-template-columns: 1fr; } .answer { grid-column: 1; } button { justify-self: stretch; } }
</style>
