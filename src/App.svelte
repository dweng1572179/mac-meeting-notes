<script lang="ts">
  import { onMount } from 'svelte';
  import {
    bootstrap,
    createSession,
    deleteSession,
    deleteTranscript,
    onSessionUpdated
  } from './lib/api';
  import LibraryView from './lib/LibraryView.svelte';
  import MeetingEditor from './lib/MeetingEditor.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import type { Session } from './lib/types';

  let sessions = $state<Session[]>([]);
  let selectedId = $state<string | null>(null);
  let selectedFolder = $state<string | null>(null);
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
        sessions = data.sessions;
        selectedId = null;
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

  async function selectLibrary(folder: string | null) {
    error = '';
    try {
      await editor?.flush();
      selectedId = null;
      selectedFolder = folder;
    } catch {
      error = 'Save this note before switching views.';
    }
  }

  async function removeTranscript(id: string) {
    if (id === selectedId) await editor?.flush();
    updateSession(await deleteTranscript(id));
  }

  async function removeMeeting(id: string) {
    if (id === selectedId) await editor?.flush();
    await deleteSession(id);
    sessions = sessions.filter((session) => session.id !== id);
    if (id === selectedId) selectedId = null;
    if (selectedFolder && !sessions.some((session) => session.folder === selectedFolder)) {
      selectedFolder = null;
    }
    const { [id]: _removed, ...active } = recordingBaselines;
    recordingBaselines = active;
  }
</script>

<div class="app-shell">
  <Sidebar
    {sessions}
    {selectedId}
    {selectedFolder}
    {creating}
    onHome={() => selectLibrary(null)}
    onNewNote={newNote}
    onSelect={selectSession}
    onFolder={(folder) => selectLibrary(folder)}
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
          onDeleteTranscript={removeTranscript}
          onDeleteMeeting={removeMeeting}
        />
      {/key}
    {:else}
      {#key selectedFolder}
        <LibraryView
          {sessions}
          {hasApiKey}
          folder={selectedFolder}
          onSelect={selectSession}
          onNewNote={newNote}
          onOpenSettings={() => (settingsOpen = true)}
        />
      {/key}
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
