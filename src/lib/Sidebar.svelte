<script lang="ts">
  import { foldersFor } from './library';
  import type { Session } from './types';

  let {
    sessions,
    selectedId,
    selectedFolder,
    creating = false,
    onHome,
    onNewNote,
    onSelect,
    onFolder,
    onSettings
  }: {
    sessions: Session[];
    selectedId: string | null;
    selectedFolder: string | null;
    creating?: boolean;
    onHome: () => void;
    onNewNote: () => void;
    onSelect: (id: string) => void;
    onFolder: (folder: string) => void;
    onSettings: () => void;
  } = $props();

  let query = $state('');
  let filteredSessions = $derived(
    sessions.filter((session) =>
      `${session.title} ${session.context}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())
    )
  );
  let folders = $derived(foldersFor(sessions));

  function sessionDate(startedAt: string) {
    return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(
      new Date(startedAt)
    );
  }
</script>

<aside class="sidebar" aria-label="Meeting library">
  <div class="sidebar-top">
    <button class="wordmark" type="button" onclick={onHome}>Meeting Notes</button>

    <label class="search">
      <span class="sr-only">Search meetings</span>
      <svg aria-hidden="true" viewBox="0 0 20 20">
        <circle cx="8.5" cy="8.5" r="5.5"></circle>
        <path d="m12.5 12.5 4 4"></path>
      </svg>
      <input bind:value={query} type="search" placeholder="Search" />
    </label>

    <nav aria-label="Primary">
      <button class="nav-item" class:active={!selectedId && selectedFolder === null} aria-current={!selectedId && selectedFolder === null ? 'page' : undefined} type="button" onclick={onHome}>
        <svg aria-hidden="true" viewBox="0 0 20 20"><path d="M3.5 9 10 3.5 16.5 9v7.5h-5v-4h-3v4h-5Z"></path></svg>
        <span>All meetings</span>
      </button>
      <button class="nav-item" type="button" disabled={creating} onclick={onNewNote}>
        <svg aria-hidden="true" viewBox="0 0 20 20"><path d="M10 4v12M4 10h12"></path></svg>
        <span>{creating ? 'Creating…' : 'New note'}</span>
      </button>
    </nav>
  </div>

  {#if folders.length}
    <nav class="folder-nav" aria-label="Folders">
      <h2>Folders</h2>
      {#each folders as folder}
        <button
          class="folder-item"
          class:active={!selectedId && selectedFolder === folder}
          aria-current={!selectedId && selectedFolder === folder ? 'page' : undefined}
          type="button"
          onclick={() => onFolder(folder)}
        >
          <svg aria-hidden="true" viewBox="0 0 20 20"><path d="M2.8 5.5h5l1.5 1.7h7.9v8.1H2.8Z"></path></svg>
          <span>{folder}</span>
        </button>
      {/each}
    </nav>
  {/if}

  <div class="recent">
    <h2>Recent</h2>
    {#if filteredSessions.length}
      <ul>
        {#each filteredSessions as session (session.id)}
          <li>
            <button
              class:selected={selectedId === session.id}
              aria-current={selectedId === session.id ? 'page' : undefined}
              type="button"
              onclick={() => onSelect(session.id)}
            >
              <span>{session.title}</span>
              <time datetime={session.startedAt}>{sessionDate(session.startedAt)}</time>
            </button>
          </li>
        {/each}
      </ul>
    {:else if query}
      <p class="sidebar-empty">No meetings match. Try another search.</p>
    {:else}
      <p class="sidebar-empty">Your recent notes will appear here.</p>
    {/if}
  </div>

  <button class="settings-button" type="button" onclick={onSettings}>
    <svg aria-hidden="true" viewBox="0 0 20 20">
      <circle cx="10" cy="10" r="2.5"></circle>
      <path d="M10 2.8v2M10 15.2v2M2.8 10h2M15.2 10h2M4.9 4.9l1.4 1.4M13.7 13.7l1.4 1.4M15.1 4.9l-1.4 1.4M6.3 13.7l-1.4 1.4"></path>
    </svg>
    <span>Settings</span>
  </button>
</aside>
