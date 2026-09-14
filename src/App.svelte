<script lang="ts">
  import { onMount } from 'svelte';
  import { bootstrap, createSession, onSessionUpdated } from './lib/api';
  import MeetingEditor from './lib/MeetingEditor.svelte';
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
  let editor = $state<{ flush: () => Promise<void> } | undefined>();
  let recordingBaselines = $state<Record<string, number>>({});
  let selected = $derived(sessions.find((session) => session.id === selectedId) ?? null);

  onMount(() => {
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

    const unlisten = onSessionUpdated(updateSession);

    return () => unlisten();
  });

  async function newNote() {
    creating = true;
    error = '';
    try {
      await editor?.flush();
      const session = await createSession({ title: 'Untitled meeting', context: '', attendees: [] });
      sessions = [session, ...sessions];
      selectedId = session.id;
    } catch {
      error = 'A new note could not be created. Check that Meeting Notes can write to its data folder.';
    } finally {
      creating = false;
    }
  }

  function updateSession(updated: Session) {
    if (updated.status !== 'recording' && recordingBaselines[updated.id] !== undefined) {
      const { [updated.id]: _finished, ...active } = recordingBaselines;
      recordingBaselines = active;
    }
    sessions = sessions.some(({ id }) => id === updated.id)
      ? sessions.map((session) => (session.id === updated.id ? updated : session))
      : [updated, ...sessions];
  }

  function rememberRecordingStart(id: string, baseline: number) {
    const current = recordingBaselines[id];
    if (current === undefined || baseline < current) {
      recordingBaselines = { ...recordingBaselines, [id]: baseline };
    }
  }

  async function selectSession(id: string | null) {
    if (id === selectedId) return;
    error = '';
    try {
      await editor?.flush();
      selectedId = id;
    } catch {
      error = 'Save this note before switching meetings.';
    }
  }
</script>

<div class="app-shell">
  <Sidebar
    {sessions}
    {selectedId}
    {creating}
    onHome={() => selectSession(null)}
    onNewNote={newNote}
    onSelect={selectSession}
    onSettings={() => (settingsOpen = true)}
  />

  <main>
    {#if error}
      <div class="app-error" role="alert">{error}</div>
    {/if}

    {#if loading}
      <p class="loading-copy">Opening your meeting library…</p>
    {:else if selected}
      {#key selected.id}
        <MeetingEditor
          bind:this={editor}
          session={selected}
          {hasApiKey}
          recordingStartedAt={recordingBaselines[selected.id] ?? null}
          onRecordingStarted={rememberRecordingStart}
          onSessionChange={updateSession}
          onOpenSettings={() => (settingsOpen = true)}
        />
      {/key}
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
  <SettingsDialog
    {hasApiKey}
    onSaved={() => (hasApiKey = true)}
    onClose={() => (settingsOpen = false)}
  />
{/if}
