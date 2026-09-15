<script lang="ts">
  import { askMeetings } from './api';
  import { sessionsForFolder } from './library';
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
  let answer = $state<MeetingAnswer | null>(null);
  let asking = $state(false);
  let askError = $state('');

  const meetingTime = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }).format(
      new Date(startedAt)
    );

  function meetingDetail(session: Session) {
    if (session.attendees.length) return session.attendees.join(', ');
    if (session.context.trim()) return session.context.trim();
    return session.status === 'complete' ? 'Meeting notes ready' : 'Draft meeting';
  }

  async function ask(event: SubmitEvent) {
    event.preventDefault();
    if (!question.trim()) return;
    if (!hasApiKey) {
      onOpenSettings();
      return;
    }
    asking = true;
    askError = '';
    answer = null;
    try {
      answer = await askMeetings(folder, question.trim());
    } catch (error) {
      askError =
        error && typeof error === 'object' && 'message' in error && typeof error.message === 'string'
          ? error.message
          : 'The meeting answer could not be created.';
    } finally {
      asking = false;
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
                  <span class:failed={session.status === 'failed'}>{session.status}</span>
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
      <p>Meeting intelligence</p>
      <h2 id="ask-title">Ask {folder ? `about ${folder}` : 'your meetings'}</h2>
      <span>Answers will cite the exact meetings they came from.</span>
    </div>
    <form class="ask-form" onsubmit={ask}>
      <label for="meeting-question">Question</label>
      <textarea
        id="meeting-question"
        bind:value={question}
        rows="3"
        placeholder="What decisions are still waiting on follow-up?"
      ></textarea>
      <button type="submit" disabled={asking || !question.trim()}>
        {asking ? 'Reading meetings…' : hasApiKey ? 'Ask meetings' : 'Add OpenAI key'}
      </button>
    </form>

    <div class="ask-response" aria-live="polite">
      {#if askError}
        <p class="ask-error" role="alert">{askError}</p>
      {:else if answer}
        <p class="answer-copy">{answer.answer}</p>
        {#if answer.citations.length}
          <ol class="citation-list" aria-label="Meeting sources">
            {#each answer.citations as citation}
              <li>
                <button type="button" onclick={() => onSelect(citation.sessionId)}>
                  <strong>{citation.title}</strong>
                  <span>“{citation.excerpt}”</span>
                </button>
              </li>
            {/each}
          </ol>
        {/if}
      {:else}
        <p class="ask-hint">Ask across the {folder ? 'meetings in this folder' : 'latest 20 completed meetings'}.</p>
      {/if}
    </div>
  </aside>
</section>
