<script lang="ts">
  import { onMount } from 'svelte';
  import { bootstrap, createSession, onSessionUpdated } from './lib/api';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import type { Session } from './lib/types';

  let sessions = $state<Session[]>([]);
  let selectedId = $state<string | null>(null);
  let hasApiKey = $state(false);
  let settingsOpen = $state(false);
  let loading = $state(true);
  let creating = $state(false);
  let error = $state('');
  let selected = $derived(sessions.find((session) => session.id === selectedId) ?? null);

  onMount(() => {
    let unlisten = () => {};

    bootstrap()
      .then((data) => {
        sessions = [...data.sessions].sort((a, b) => b.startedAt.localeCompare(a.startedAt));
        selectedId = sessions[0]?.id ?? null;
        hasApiKey = data.hasApiKey;
      })
      .catch(() => {
        error = 'Meeting Notes could not load your library. Quit and reopen the app to try again.';
      })
      .finally(() => (loading = false));

    onSessionUpdated((updated) => {
      sessions = sessions.some(({ id }) => id === updated.id)
        ? sessions.map((session) => (session.id === updated.id ? updated : session))
        : [updated, ...sessions];
    }).then((stop) => (unlisten = stop));

    return () => unlisten();
  });

  async function newNote() {
    creating = true;
    error = '';
    try {
      const session = await createSession({ title: 'Untitled meeting', context: '', attendees: [] });
      sessions = [session, ...sessions];
      selectedId = session.id;
    } catch {
      error = 'A new note could not be created. Check that Meeting Notes can write to its data folder.';
    } finally {
      creating = false;
    }
  }

  const meetingDate = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(
      new Date(startedAt)
    );
</script>

<div class="app-shell">
  <Sidebar
    {sessions}
    {selectedId}
    {creating}
    onHome={() => (selectedId = null)}
    onNewNote={newNote}
    onSelect={(id) => (selectedId = id)}
    onSettings={() => (settingsOpen = true)}
  />

  <main>
    {#if error}
      <div class="app-error" role="alert">{error}</div>
    {/if}

    {#if loading}
      <p class="loading-copy">Opening your meeting library…</p>
    {:else if selected}
      <article class="meeting-document" aria-labelledby="meeting-title">
        <header class="document-header">
          <p class="meeting-date">{meetingDate(selected.startedAt)}</p>
          <h1 id="meeting-title">{selected.title}</h1>
          <div class="meeting-meta">
            <span class:failed={selected.status === 'failed'}>{selected.status}</span>
            {#if selected.attendees.length}
              <span>{selected.attendees.join(', ')}</span>
            {/if}
          </div>
          {#if selected.context}
            <p class="meeting-context">{selected.context}</p>
          {/if}
        </header>

        <section class="notes-section" aria-labelledby="notes-title">
          <h2 id="notes-title">Notes</h2>
          {#if selected.originalNotes}
            <div class="note-copy">{selected.originalNotes}</div>
          {:else}
            <p class="document-empty">Start with the questions, numbers, and decisions you want to remember.</p>
          {/if}
        </section>

        {#if selected.enrichedNotes}
          <section class="notes-section" aria-labelledby="summary-title">
            <h2 id="summary-title">Summary</h2>
            <div class="note-copy enriched">{selected.enrichedNotes}</div>
          </section>
        {/if}

        {#if selected.error}
          <p class="processing-error" role="alert">{selected.error.message} Open the note and retry processing.</p>
        {/if}
      </article>
    {:else}
      <section class="library-empty" aria-labelledby="empty-title">
        <p>{sessions.length ? 'Meeting library' : 'Your meeting library'}</p>
        <h1 id="empty-title">{sessions.length ? 'Choose a note to continue.' : 'Make the call easier to remember.'}</h1>
        <p>{sessions.length ? 'Select a recent meeting from the sidebar.' : 'Create a note before your next meeting. It will stay here when the call ends.'}</p>
        {#if !sessions.length}
          <button type="button" onclick={newNote}>New note</button>
        {/if}
      </section>
    {/if}
  </main>
</div>

{#if settingsOpen}
  <SettingsDialog {hasApiKey} onClose={() => (settingsOpen = false)} />
{/if}
