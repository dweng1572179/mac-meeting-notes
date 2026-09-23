<script lang="ts">
  import { tick } from 'svelte';
  import AnswerBody from './AnswerBody.svelte';
  import QuestionComposer from './QuestionComposer.svelte';
  import { questionConversations, type AskQuestion } from './questions';

  let { id, scope, sourceIds, hasApiKey, onAsk, onOpenSettings, onSelectSource, starters = [] }: {
    id: string;
    scope: string;
    sourceIds: string[];
    hasApiKey: boolean;
    onAsk: AskQuestion;
    onOpenSettings: () => void;
    onSelectSource?: (id: string) => void;
    starters?: string[];
  } = $props();

  let conversation = $derived(questionConversations.get(scope));
  let thread = $derived($conversation);
  let exchangeCount = $derived(thread.exchanges.length);
  let asking = $derived(thread.asking);
  let composer: QuestionComposer;
  let history: HTMLDivElement;
  let copyStatus = $state('');
  let copyFailed = $state(false);

  $effect(() => {
    exchangeCount;
    asking;
    void tick().then(() => history?.scrollTo({ top: history.scrollHeight }));
  });

  async function ask() {
    if (!hasApiKey) { onOpenSettings(); return; }
    await conversation.ask(onAsk, sourceIds);
  }

  async function retry() {
    if (!hasApiKey) { onOpenSettings(); return; }
    await conversation.retry(onAsk, sourceIds);
  }

  async function copyAnswer(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copyStatus = 'Answer copied.';
      copyFailed = false;
    } catch {
      copyStatus = 'The answer could not be copied. Select its text and copy it manually.';
      copyFailed = true;
    }
  }

  function chooseStarter(question: string) {
    conversation.edit(question);
    composer.focus();
  }
</script>

<div class="question-thread">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (The scrollable conversation needs keyboard focus for arrow/Page Up navigation.) -->
  <div class="thread-history" bind:this={history} tabindex="0" role="region" aria-label="Conversation">
  {#if thread.exchanges.length}
    <ol class="exchange-list" aria-label="Questions and answers">
      {#each thread.exchanges as exchange}
        <li class="exchange">
          <p class="asked" dir="auto">{exchange.question}</p>
          <AnswerBody text={exchange.answer.answer} />
          <button class="copy-answer" type="button" onclick={() => copyAnswer(exchange.answer.answer)}
            aria-label={`Copy answer to: ${exchange.question}`}>Copy answer</button>
          {#if exchange.answer.citations.length}
            <details class="answer-sources">
              <summary>{exchange.answer.citations.length} {exchange.answer.citations.length === 1 ? 'source' : 'sources'}</summary>
              <ol aria-label="Exact meeting sources">
                {#each exchange.answer.citations as citation}
                  <li>
                    {#if onSelectSource}
                      <button class="source-link" type="button" onclick={() => onSelectSource?.(citation.sessionId)}>
                        <span dir="auto">{citation.title || 'Untitled meeting'}</span>
                        <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M4 8h8M9 4l4 4-4 4" /></svg>
                      </button>
                    {/if}
                    <blockquote dir="auto">{citation.excerpt}</blockquote>
                  </li>
                {/each}
              </ol>
            </details>
          {/if}
        </li>
      {/each}
    </ol>
  {:else if starters.length}
    <ul class="question-starters" aria-label="Suggested questions">
      {#each starters as starter}
        <li><button type="button" disabled={thread.asking} onclick={() => chooseStarter(starter)}>{starter}</button></li>
      {/each}
    </ul>
  {/if}

  {#if thread.asking && thread.pendingQuestion}
    <p class="pending-question" dir="auto">{thread.pendingQuestion}</p>
  {/if}
  {#if thread.error}
    <div class="question-error" role="alert">
      <p>{thread.error}</p>
      {#if thread.pendingQuestion}<p class="retry-question" dir="auto">{thread.pendingQuestion}</p>{/if}
      <button type="button" onclick={retry} disabled={thread.asking || !(thread.pendingQuestion || thread.question.trim())}>Try again</button>
    </div>
  {/if}
  {#if thread.persistenceError}
    <div class="question-error" role="alert">
      <p>{thread.persistenceError}</p>
      <button type="button" onclick={() => conversation.retrySave()}>Retry saving</button>
    </div>
  {/if}
  {#if copyStatus}<p class="copy-status" class:copy-failed={copyFailed} role="status">{copyStatus}</p>{/if}
  </div>
  <div class="thread-composer">
    <QuestionComposer bind:this={composer} {id} question={thread.question} asking={thread.asking} {hasApiKey}
      placeholder={thread.exchanges.length ? 'Ask a follow-up…' : 'Ask a question…'}
      onAsk={ask} onInput={(question) => conversation.edit(question)} />
    {#if thread.exchanges.length || (thread.pendingQuestion && !thread.asking)}
      <button class="new-conversation" type="button" disabled={thread.asking}
        onclick={() => { conversation.clear(); composer.focus(); }}>New conversation</button>
    {/if}
  </div>
  <p class="sr-only" role="status">{thread.asking ? 'Reading your notes.' : thread.exchanges.length ? 'Answer ready.' : ''}</p>
</div>

<style>
  .question-thread { display: flex; flex: 1; flex-direction: column; min-width: 0; min-height: 0; }
  .thread-history { flex: 1; min-height: 0; padding: 2px 4px 16px 2px; overflow-y: auto; overscroll-behavior: contain; scrollbar-gutter: stable; }
  .thread-history:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .thread-composer { flex: 0 0 auto; padding-top: 12px; border-top: 1px solid var(--line); }
  .pending-question { margin: 0 0 12px; color: var(--ink); font-size: 14px; line-height: 1.55; overflow-wrap: anywhere; }
  .retry-question { color: var(--muted-text); }
  .copy-answer { padding: 4px 0; margin-top: 10px; border: 0; background: transparent; color: var(--muted-text); font-size: 12px; cursor: pointer; }
  .copy-answer:hover { color: var(--ink); text-decoration: underline; text-underline-offset: 3px; }
  .copy-status { color: var(--muted-text); font-size: 12px; line-height: 1.6; }
  .copy-failed { color: var(--danger); }
  .exchange-list { display: grid; gap: 28px; padding: 0; margin: 0 0 24px; list-style: none; }
  .exchange + .exchange { padding-top: 24px; border-top: 1px solid var(--line); }
  .asked { margin: 0 0 11px; color: var(--ink); font-size: 14px; font-weight: 650; line-height: 1.55; overflow-wrap: anywhere; }
  .answer-sources { margin-top: 12px; color: var(--muted-text); font-size: 12px; }
  summary { width: fit-content; padding: 3px 0; cursor: pointer; }
  summary:hover { color: var(--ink); }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .answer-sources ol { display: grid; gap: 13px; padding: 0; margin: 11px 0 0; list-style: none; }
  .source-link { display: inline-flex; align-items: center; gap: 7px; max-width: 100%; padding: 2px 0; border: 0; color: var(--accent-text); background: transparent; text-align: start; font-size: 12px; font-weight: 600; cursor: pointer; }
  .source-link span { overflow-wrap: anywhere; }
  .source-link:hover span { text-decoration: underline; text-underline-offset: 3px; }
  svg { flex: 0 0 14px; width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.3; }
  blockquote { margin: 5px 0 0; padding-inline-start: 12px; border-inline-start: 1px solid var(--line); line-height: 1.6; overflow-wrap: anywhere; }
  .question-starters { display: grid; gap: 3px; padding: 0; margin: 0 0 16px; list-style: none; }
  .question-starters button { padding: 5px 0; border: 0; background: transparent; color: var(--muted-text); text-align: start; font-size: 13px; line-height: 1.5; cursor: pointer; }
  .question-starters button:hover:not(:disabled) { color: var(--accent-text); text-decoration: underline; text-underline-offset: 4px; }
  .question-starters button:disabled { opacity: .5; cursor: default; }
  .question-error { margin-top: 8px; color: var(--danger); font-size: 12px; line-height: 1.6; }
  .question-error p { margin: 0; overflow-wrap: anywhere; }
  .question-error button { margin-top: 5px; padding: 4px 0; border: 0; background: transparent; color: var(--ink); font-size: 12px; font-weight: 600; cursor: pointer; text-decoration: underline; text-underline-offset: 3px; }
  .question-error button:disabled { opacity: .5; cursor: default; }
  .new-conversation { margin-top: 4px; padding: 4px 0; border: 0; background: transparent; color: var(--muted-text); font-size: 12px; cursor: pointer; }
  .new-conversation:hover:not(:disabled) { color: var(--ink); text-decoration: underline; text-underline-offset: 3px; }
  .new-conversation:disabled { opacity: .5; cursor: default; }
</style>
