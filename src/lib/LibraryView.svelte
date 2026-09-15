<script lang="ts">
  import { sessionsForFolder } from './library';
  import type { Session } from './types';

  let {
    sessions,
    folder,
    onSelect,
    onNewNote
  }: {
    sessions: Session[];
    folder: string | null;
    onSelect: (id: string) => void;
    onNewNote: () => void;
  } = $props();

  let visibleSessions = $derived(sessionsForFolder(sessions, folder));

  const meetingTime = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }).format(
      new Date(startedAt)
    );

  function meetingDetail(session: Session) {
    if (session.attendees.length) return session.attendees.join(', ');
    if (session.context.trim()) return session.context.trim();
    return session.status === 'complete' ? 'Meeting notes ready' : 'Draft meeting';
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
    <div class="ask-placeholder" aria-hidden="true">
      <span>“What decisions are still waiting on follow-up?”</span>
    </div>
  </aside>
</section>
