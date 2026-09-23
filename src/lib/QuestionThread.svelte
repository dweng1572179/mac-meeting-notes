<script lang="ts">
  import { onDestroy } from 'svelte';
  import AnswerBody from './AnswerBody.svelte';
  import QuestionComposer from './QuestionComposer.svelte';
  import { createQuestionConversation, type AskQuestion } from './questions';

  let { id, hasApiKey, onAsk, onOpenSettings, onSelectSource, starters = [] }: {
    id: string;
    hasApiKey: boolean;
    onAsk: AskQuestion;
    onOpenSettings: () => void;
    onSelectSource?: (id: string) => void;
    starters?: string[];
  } = $props();

  const conversation = createQuestionConversation((next) => { state = next; });
  let state = $state(conversation.state);
  let composer: QuestionComposer;
  onDestroy(() => conversation.dispose());

  async function ask() {
    if (!hasApiKey) { onOpenSettings(); return; }
    await conversation.ask(onAsk);
  }

  function chooseStarter(question: string) {
    conversation.edit(question);
    composer.focus();
  }
</script>

<div class="question-thread">
  {#if state.exchanges.length}
    <ol class="exchange-list" aria-label="Questions and answers">
      {#each state.exchanges as exchange}
        <li class="exchange">
          <p class="asked" dir="auto">{exchange.question}</p>
          <AnswerBody text={exchange.answer.answer} />
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
        <li><button type="button" disabled={state.asking} onclick={() => chooseStarter(starter)}>{starter}</button></li>
      {/each}
    </ul>
  {/if}

  <QuestionComposer bind:this={composer} {id} question={state.question} asking={state.asking} {hasApiKey}
    placeholder={state.exchanges.length ? 'Ask a follow-up…' : 'Ask a question…'}
    onAsk={ask} onInput={(question) => conversation.edit(question)} />
  {#if state.exchanges.length}
    <button class="new-conversation" type="button" disabled={state.asking}
      onclick={() => { conversation.clear(); composer.focus(); }}>New conversation</button>
  {/if}
  {#if state.error}
    <div class="question-error" role="alert">
      <p>{state.error}</p>
      <button type="button" onclick={ask} disabled={state.asking || !state.question.trim()}>Try again</button>
    </div>
  {/if}
  <p class="sr-only" role="status">{state.asking ? 'Reading your notes.' : state.exchanges.length ? 'Answer ready.' : ''}</p>
</div>

<style>
  .question-thread { min-width: 0; }
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
