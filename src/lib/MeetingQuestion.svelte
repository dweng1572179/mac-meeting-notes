<script lang="ts">
  import { parseMeetingMarkdown } from './markdown';
  import { errorMessage } from './recovery';
  import type { MeetingAnswer } from './types';

  let { hasApiKey, onAsk, onOpenSettings }: {
    hasApiKey: boolean;
    onAsk: (question: string) => Promise<MeetingAnswer>;
    onOpenSettings: () => void;
  } = $props();

  let question = $state('');
  let exchanges = $state<{ question: string; answer: MeetingAnswer }[]>([]);
  let error = $state('');
  let asking = $state(false);

  async function ask(event?: SubmitEvent) {
    event?.preventDefault();
    if (asking) return;
    const submitted = question.trim();
    if (!submitted) return;
    if (!hasApiKey) { onOpenSettings(); return; }
    asking = true;
    error = '';
    try {
      const answer = await onAsk(submitted);
      exchanges = [...exchanges, { question: submitted, answer }];
      question = '';
    } catch (caught) {
      error = errorMessage(caught, 'This meeting could not be read. Your question is still here; try again.');
    } finally {
      asking = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void ask();
    }
  }
</script>

<section class="meeting-question" aria-labelledby="meeting-question-title">
  <header>
    <h2 id="meeting-question-title">Ask about this meeting</h2>
    <p>Answers stay grounded in this meeting’s saved text.</p>
  </header>

  {#if exchanges.length}
    <ol class="exchange-list" aria-label="Questions and answers">
      {#each exchanges as exchange}
        <li class="exchange">
          <p class="asked" dir="auto">{exchange.question}</p>
          <div class="answer-copy" dir="auto">
            {#each parseMeetingMarkdown(exchange.answer.answer) as block}
              {#if block.kind === 'heading'}<h3>{block.text}</h3>
              {:else if block.kind === 'bullet'}<p class="answer-bullet"><span aria-hidden="true">•</span>{block.text}</p>
              {:else}<p>{block.text}</p>{/if}
            {/each}
          </div>
          {#if exchange.answer.citations.length}
            <details class="answer-sources">
              <summary>Sources ({exchange.answer.citations.length})</summary>
              <ol aria-label="Exact meeting sources">
                {#each exchange.answer.citations as citation}
                  <li><blockquote dir="auto">{citation.excerpt}</blockquote></li>
                {/each}
              </ol>
            </details>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}

  <form onsubmit={ask}>
    <label class="sr-only" for="single-meeting-question">Question</label>
    <textarea id="single-meeting-question" bind:value={question} oninput={() => (error = '')} onkeydown={handleKeydown} rows="2" maxlength="2000" dir="auto" disabled={asking} placeholder="Ask about a decision, number, or next step…"></textarea>
    <button type="submit" disabled={asking || !question.trim()}>{asking ? 'Reading…' : hasApiKey ? 'Ask' : 'Add key'}</button>
  </form>
  <p class="composer-help">⌘ Enter to ask · exact excerpts stay available under Sources</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .meeting-question { max-width: 720px; padding-block: 28px 10px; border-top: 1px solid var(--line); }
  header { display: flex; align-items: baseline; justify-content: space-between; gap: 20px; margin-bottom: 15px; }
  h2 { margin: 0; font-family: Georgia, ui-serif, serif; font-size: 20px; font-weight: 400; }
  header p, .composer-help { margin: 0; color: var(--muted-text); font-size: 11px; line-height: 1.45; }
  form { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: end; gap: 8px; padding: 7px; border: 1px solid var(--line); border-radius: 12px; background: color-mix(in srgb, var(--paper) 72%, var(--sidebar)); }
  textarea { width: 100%; min-height: 46px; max-height: 150px; resize: vertical; padding: 8px 9px; border: 0; outline: 0; color: var(--ink); background: transparent; font: inherit; font-size: 15px; line-height: 1.45; }
  textarea::placeholder { color: var(--muted-text); }
  button { min-width: 62px; min-height: 36px; padding: 0 13px; border: 0; border-radius: 8px; color: #fff; background: var(--accent-text); font-weight: 700; cursor: pointer; }
  button:disabled { cursor: not-allowed; opacity: .48; }
  form:focus-within { outline: 2px solid color-mix(in srgb, var(--accent) 52%, transparent); outline-offset: 2px; }
  .composer-help { margin-top: 6px; text-align: right; }
  .error { margin: 10px 0 0; color: var(--danger); font-size: 12px; line-height: 1.5; }
  .exchange-list { display: grid; gap: 22px; padding: 0; margin: 0 0 20px; list-style: none; }
  .exchange { animation: answer-in 180ms cubic-bezier(.22, 1, .36, 1); }
  .asked { margin: 0 0 7px; color: var(--muted-text); font-size: 12px; font-weight: 650; }
  .asked::before { content: 'You asked · '; color: var(--accent-text); }
  .answer-copy { max-width: 68ch; color: #3e3e36; font-family: Georgia, ui-serif, serif; font-size: 15px; line-height: 1.7; }
  .answer-copy h3 { margin: 20px 0 7px; font-size: 17px; }
  .answer-copy p { margin: 0 0 8px; }
  .answer-bullet { display: grid; grid-template-columns: 17px 1fr; }
  .answer-bullet span { color: var(--accent-text); }
  .answer-sources { margin-top: 9px; color: var(--muted-text); font-size: 11px; }
  .answer-sources summary { width: fit-content; cursor: pointer; font-weight: 650; }
  .answer-sources ol { display: grid; gap: 7px; padding: 0; margin: 9px 0 0; list-style: none; }
  blockquote { margin: 0; padding: 9px 11px; border: 1px solid var(--line); border-radius: 9px; background: color-mix(in srgb, var(--paper) 70%, transparent); line-height: 1.5; overflow-wrap: anywhere; }
  @keyframes answer-in { from { opacity: 0; transform: translateY(4px); } }
  @media (prefers-reduced-motion: reduce) { .exchange { animation: none; } }
  @media (max-width: 680px) { header { align-items: flex-start; flex-direction: column; gap: 4px; } .composer-help { text-align: left; } }
</style>
