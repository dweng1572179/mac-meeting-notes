<script lang="ts">
  import { parseMeetingMarkdown } from './markdown';
  import InlineMarkdown from './InlineMarkdown.svelte';
  import QuestionComposer from './QuestionComposer.svelte';
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
</script>

<section class="meeting-question" aria-labelledby="meeting-question-title">
  <header>
    <h2 id="meeting-question-title">Ask this meeting</h2>
    <p>From your notes and transcript</p>
  </header>

  {#if exchanges.length}
    <ol class="exchange-list" aria-label="Questions and answers">
      {#each exchanges as exchange}
        <li class="exchange">
          <p class="asked" dir="auto">{exchange.question}</p>
          <div class="answer-copy" dir="auto">
            {#each parseMeetingMarkdown(exchange.answer.answer) as block}
              {#if block.kind === 'heading'}<h3><InlineMarkdown text={block.text} /></h3>
              {:else if block.kind === 'bullet'}<p class="answer-bullet"><span aria-hidden="true">•</span><InlineMarkdown text={block.text} /></p>
              {:else}<p><InlineMarkdown text={block.text} /></p>{/if}
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

  <QuestionComposer id="single-meeting-question" bind:question {asking} {hasApiKey} onAsk={ask} onInput={() => (error = '')} />
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .meeting-question { max-width: 720px; padding-block: 26px 10px; border-top: 1px solid var(--line); }
  header { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  h2 { margin: 0; font-size: 15px; font-weight: 600; letter-spacing: -.015em; }
  header p { margin: 0; color: var(--muted-text); font-size: 12px; line-height: 1.5; }
  .error { margin: 10px 0 0; color: var(--danger); font-size: 12px; line-height: 1.5; }
  .exchange-list { display: grid; gap: 22px; padding: 0; margin: 0 0 20px; list-style: none; }
  .exchange { animation: answer-in 180ms cubic-bezier(.22, 1, .36, 1); }
  .asked { margin: 0 0 7px; color: var(--muted-text); font-size: 12px; font-weight: 650; }
  .answer-copy { max-width: 68ch; color: #3e3e36; font-size: 14px; line-height: 1.7; overflow-wrap: anywhere; }
  .answer-copy h3 { margin: 20px 0 7px; font-size: 17px; }
  .answer-copy p { margin: 0 0 8px; }
  .answer-bullet { display: grid; grid-template-columns: 17px 1fr; }
  .answer-bullet > span[aria-hidden] { color: var(--accent-text); }
  .answer-sources { margin-top: 9px; color: var(--muted-text); font-size: 11px; }
  .answer-sources summary { width: fit-content; cursor: pointer; font-weight: 650; }
  .answer-sources ol { display: grid; gap: 7px; padding: 0; margin: 9px 0 0; list-style: none; }
  blockquote { margin: 0; padding: 4px 0 4px 12px; border-left: 1px solid var(--line); line-height: 1.5; overflow-wrap: anywhere; }
  @keyframes answer-in { from { opacity: 0; transform: translateY(4px); } }
  @media (prefers-reduced-motion: reduce) { .exchange { animation: none; } }
  @media (max-width: 680px) { header { align-items: flex-start; flex-direction: column; gap: 4px; } }
</style>
