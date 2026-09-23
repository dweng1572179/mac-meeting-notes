<script lang="ts">
  import { askMeetings } from './api';
  import { sessionsForFolder, meetingStatusLabel } from './library';
  import QuestionThread from './QuestionThread.svelte';
  import type { Session } from './types';

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
  let chatExpanded = $state(false);

  const meetingTime = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }).format(
      new Date(startedAt)
    );

  function meetingDetail(session: Session) {
    if (session.attendees.length) return session.attendees.join(', ');
    if (session.context.trim()) return session.context.trim();
    return meetingStatusLabel(session);
  }

</script>

<section class="library-workspace" aria-labelledby="library-title">
  <div class="library-column">
    <header class="library-header">
      <div class="library-title-copy">
        {#if folder}
          <span class="folder-mark" aria-hidden="true"><svg viewBox="0 0 20 20"><path d="M2.5 5.5h5l2 2h8v8h-15z" /></svg></span>
        {/if}
        <div>
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

  <aside class="ask-panel question-panel" class:expanded={chatExpanded} aria-label={folder ? `Ask about ${folder}` : 'Ask your meetings'}>
    <button class="question-panel-toggle" type="button" aria-expanded={chatExpanded} aria-controls="library-question-panel" onclick={() => (chatExpanded = !chatExpanded)}>
      <span>Ask {folder ? 'this folder' : 'your meetings'}</span>
      <svg aria-hidden="true" viewBox="0 0 20 20"><path d="m5 12 5-5 5 5" /></svg>
    </button>
    <div class="question-panel-content" id="library-question-panel">
      <header class="question-panel-heading">
        <h2>Ask {folder ? `about ${folder}` : 'your meetings'}</h2>
        <p>{folder ? 'From the latest 20 meetings in this folder' : 'From your latest 20 completed meetings'}</p>
      </header>
      {#key folder}
        <QuestionThread id="meeting-question" scope={folder === null ? 'all' : `folder:${folder}`} sourceIds={visibleSessions.filter((session) => session.status !== 'draft').map((session) => session.id)} {hasApiKey} {onOpenSettings}
          onAsk={(question, history) => askMeetings(folder, question, history)} onSelectSource={onSelect}
          starters={['Summarize the main themes.', 'What changed across these meetings?']} />
      {/key}
    </div>
  </aside>
</section>
