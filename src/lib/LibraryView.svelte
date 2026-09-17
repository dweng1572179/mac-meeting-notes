<script lang="ts">
  import { askMeetings } from './api';
  import { sessionsForFolder, meetingStatusLabel } from './library';
  import { parseMeetingMarkdown } from './markdown';
  import { errorMessage } from './recovery';
  import type { MeetingAnswer, Session } from './types';

  let {
    sessions,
    folder,
    hasApiKey,
    onSelect,
    onNewNote,
    onOpenSettings
  }: {
    sessions: Session[];
    folder: string | null;
    hasApiKey: boolean;
    onSelect: (id: string) => void;
    onNewNote: () => void;
    onOpenSettings: () => void;
  } = $props();

  let visibleSessions = $derived(sessionsForFolder(sessions, folder));
  let question = $state('');
  let exchanges = $state<{ question: string; answer: MeetingAnswer }[]>([]);
  let asking = $state(false);
  let askError = $state('');

  const meetingTime = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }).format(
      new Date(startedAt)
    );

  function meetingDetail(session: Session) {
    if (session.attendees.length) return session.attendees.join(', ');
    if (session.context.trim()) return session.context.trim();
    return meetingStatusLabel(session);
  }

  async function ask(event?: SubmitEvent) {
    event?.preventDefault();
    if (asking) return;
    const submitted = question.trim();
    if (!submitted) return;
    if (!hasApiKey) {
      onOpenSettings();
      return;
    }
    asking = true;
    askError = '';
    try {
      const answer = await askMeetings(folder, submitted);
      exchanges = [...exchanges, { question: submitted, answer }];
      question = '';
    } catch (error) {
      askError = errorMessage(error, 'The meeting answer could not be created. Your question is still here; try again.');
    } finally {
      asking = false;
    }
  }

  function handleQuestionKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void ask();
    }
  }
</script>

<section class="library-workspace" aria-labelledby="library-title">
  <div class="library-column">
    <header class="library-header">
      <div class="library-title-copy">
        {#if folder}
          <span class="folder-mark" aria-hidden="true">↗</span>
        {/if}
        <div>
          <p>{folder ? 'Folder' : 'Local meeting library'}</p>
          <h1 id="library-title">{folder ?? 'All meetings'}</h1>
          <span>{folder ? `Meetings filed under ${folder}` : 'Every conversation, note, and decision in one place.'}</span>
        </div>
      </div>
      <button class="new-note-button" type="button" onclick={onNewNote}>New note</button>
    </header>

    {#if visibleSessions.length}
      <ol class="meeting-timeline" aria-label={folder ? `${folder} meetings` : 'All meetings'}>
        {#each visibleSessions as session (session.id)}
          <li>
            <button type="button" onclick={() => onSelect(session.id)}>
              <span class="meeting-row-main">
                <strong>{session.title || 'Untitled meeting'}</strong>
                <span>{meetingDetail(session)}</span>
              </span>
              <span class="meeting-row-meta">
                <time datetime={session.startedAt}>{meetingTime(session.startedAt)}</time>
                {#if session.status !== 'complete'}
                  <span class:failed={session.status === 'failed' && session.error?.code !== 'no_speech'}>{meetingStatusLabel(session)}</span>
                {/if}
              </span>
            </button>
          </li>
        {/each}
      </ol>
    {:else}
      <div class="timeline-empty">
        <h2>{folder ? 'No meetings in this folder.' : 'Your first meeting starts here.'}</h2>
        <p>{folder ? 'Add this folder name to a meeting note to place it here.' : 'Create a note, start recording, and keep typing only what matters to you.'}</p>
        <button type="button" onclick={onNewNote}>Create a note</button>
      </div>
    {/if}
  </div>

  <aside class="ask-panel" aria-labelledby="ask-title">
    <div>
      <h2 id="ask-title">Ask {folder ? `about ${folder}` : 'your meetings'}</h2>
      <span>Answers will cite the exact meetings they came from.</span>
    </div>
    <div class="ask-response" aria-live="polite">
      {#if exchanges.length}
        <ol class="library-exchanges" aria-label="Questions and answers">
          {#each exchanges as exchange}
            <li>
              <p class="asked-question" dir="auto">{exchange.question}</p>
              <div class="answer-copy" dir="auto">
                {#each parseMeetingMarkdown(exchange.answer.answer) as block}
                  {#if block.kind === 'heading'}<h3>{block.text}</h3>
                  {:else if block.kind === 'bullet'}<p class="answer-bullet"><span aria-hidden="true">•</span>{block.text}</p>
                  {:else}<p>{block.text}</p>{/if}
                {/each}
              </div>
              {#if exchange.answer.citations.length}
                <details class="library-sources">
                  <summary>Sources ({exchange.answer.citations.length})</summary>
                  <ol class="citation-list" aria-label="Meeting sources">
                    {#each exchange.answer.citations as citation}
                      <li><button type="button" onclick={() => onSelect(citation.sessionId)}><strong>{citation.title}</strong><span>“{citation.excerpt}”</span></button></li>
                    {/each}
                  </ol>
                </details>
              {/if}
            </li>
          {/each}
        </ol>
      {/if}
    </div>
    <form class="ask-form compact" onsubmit={ask}>
      <label class="sr-only" for="meeting-question">Question</label>
      <textarea
        id="meeting-question"
        bind:value={question}
        maxlength="2000"
        disabled={asking}
        dir="auto"
        oninput={() => (askError = '')}
        onkeydown={handleQuestionKeydown}
        rows="2"
        placeholder="Ask across these meetings…"
      ></textarea>
      <button type="submit" disabled={asking || !question.trim()}>
        {asking ? 'Reading…' : hasApiKey ? 'Ask' : 'Add key'}
      </button>
    </form>
    <p class="ask-hint">⌘ Enter to ask · uses up to the latest 20 completed meetings{folder ? ' in this folder' : ''}</p>
    {#if askError}<p class="ask-error" role="alert">{askError}</p>{/if}
  </aside>
</section>
