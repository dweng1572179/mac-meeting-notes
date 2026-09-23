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
      <span>{folder ? 'From the latest 20 meetings in this folder' : 'From your latest 20 completed meetings'}</span>
    </div>
    {#key folder}
      <QuestionThread id="meeting-question" scope={folder === null ? 'all' : `folder:${folder}`} sourceIds={visibleSessions.filter((session) => session.status !== 'draft').map((session) => session.id)} {hasApiKey} {onOpenSettings}
        onAsk={(question, history) => askMeetings(folder, question, history)} onSelectSource={onSelect}
        starters={['Summarize the main themes.', 'What changed across these meetings?']} />
    {/key}
  </aside>
</section>
