<script lang="ts">
  import { onMount } from 'svelte';
  import {
    bootstrap,
    createSession,
    saveSession,
    defaultTranscriptionSettings,
    deleteSession,
    deleteTranscript,
    recordingHealth,
    onSessionUpdated
  } from './lib/api';
  import LibraryView from './lib/LibraryView.svelte';
  import MeetingEditor from './lib/MeetingEditor.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import { errorMessage } from './lib/recovery';
  import { questionConversations } from './lib/questions';
  import type { RecordingHealth, Session, TranscriptionSettings } from './lib/types';
  import { captureCoverage } from './lib/RecordingDock.svelte';

  let sessions = $state<Session[]>([]);
  let selectedId = $state<string | null>(null);
  let selectedFolder = $state<string | null>(null);
  let hasApiKey = $state(false);
  let settings = $state<TranscriptionSettings>({ ...defaultTranscriptionSettings });
  let loadFailed = $state(false);
  let settingsOpen = $state(false);
  let loading = $state(true);
  let creating = $state(false);
  let error = $state('');
  let health = $state<RecordingHealth | null>(null);
  let healthError = $state('');
  let recordingId = $derived(sessions.find((session) => session.status === 'recording')?.id ?? null);

  $effect(() => {
    const id = recordingId;
    health = null;
    healthError = '';
    if (!id) return;
    let disposed = false;
    let pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await recordingHealth(id);
        if (!disposed) { health = next; healthError = ''; }
      } catch {
        if (!disposed) healthError = 'Capture health is unavailable. Recording coverage cannot be verified.';
      } finally { pending = false; }
    };
    void refresh();
    const timer = window.setInterval(refresh, 2_000);
    return () => { disposed = true; window.clearInterval(timer); };
  });
  let editor = $state<{ flush: () => Promise<void> } | undefined>();
  let recordingBaselines = $state<Record<string, number>>({});
  let selected = $derived(sessions.find((session) => session.id === selectedId) ?? null);
  let availableFolders = $derived([...new Set(sessions.map((session) => session.folder).filter(Boolean))].sort());

  async function loadLibrary() {
    loading = true;
    error = '';
    try {
      const data = await bootstrap();
      sessions = data.sessions;
      hasApiKey = data.hasApiKey;
      settings = data.settings ?? { ...defaultTranscriptionSettings };
      loadFailed = false;
    } catch (cause) {
      loadFailed = true;
      error = errorMessage(cause, 'Meeting Notes could not load your library. Try again.');
    } finally { loading = false; }
  }

  onMount(() => {
    void loadLibrary();
    return onSessionUpdated(updateSession);
  });

  async function newNote() {
    if (creating) return;
    creating = true;
    error = '';
    try {
      await editor?.flush();
      let session = await createSession({ title: 'Untitled meeting', context: '', attendees: [] });
      if (selectedFolder) session = await saveSession({ id: session.id, title: session.title, context: '', attendees: [], folder: selectedFolder, originalNotes: '' });
      sessions = [session, ...sessions];
      selectedId = session.id;
    } catch (cause) {
      error = errorMessage(cause, 'A new note could not be created. Your current note is still open.');
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
    } catch (cause) {
      error = errorMessage(cause, 'Save this note before switching meetings.');
    }
  }

  async function selectLibrary(folder: string | null) {
    error = '';
    try {
      await editor?.flush();
      selectedId = null;
      selectedFolder = folder;
    } catch (cause) {
      error = errorMessage(cause, 'Save this note before switching views.');
    }
  }

  async function removeTranscript(id: string) {
    if (id === selectedId) await editor?.flush();
    questionConversations.deleteForMeeting(id);
    updateSession(await deleteTranscript(id));
  }

  async function removeMeeting(id: string) {
    if (id === selectedId) await editor?.flush();
    questionConversations.deleteForMeeting(id);
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
    {#if recordingId}
      <aside class="capture-health" aria-label="Audio capture status">
        <p>{health ? captureCoverage(health) : 'Checking audio capture…'}</p>
        {#if healthError || health?.warnings.length}
          <div role="alert">
            {#if healthError}<p>{healthError}</p>{/if}
            {#each health?.warnings ?? [] as warning}<p>{warning}</p>{/each}
            <p>A quiet source may produce no frames. If speech is expected, check your selected microphone and system audio permissions. Stop recording before updating the app.</p>
          </div>
        {/if}
        {#if selectedId !== recordingId}
          <button type="button" onclick={() => selectSession(recordingId)}>Return to recording</button>
        {/if}
      </aside>
    {/if}
    {#if error}
      <div class="app-error" role="alert">{error}{#if loadFailed}<button type="button" disabled={loading} onclick={loadLibrary}>Reload library</button>{/if}</div>
    {/if}

    {#if loading}
      <p class="loading-copy">Opening your meeting library…</p>
    {:else if selected}
      {#key selected.id}
        <MeetingEditor
          bind:this={editor}
          session={selected}
          {availableFolders}
          {hasApiKey}
          health={selected.id === recordingId ? health : null}
          recordingStartedAt={recordingBaselines[selected.id] ?? null}
          onRecordingStarted={rememberRecordingStart}
          onSessionChange={updateSession}
          onOpenSettings={() => (settingsOpen = true)}
          onDeleteTranscript={removeTranscript}
          onDeleteMeeting={removeMeeting}
          onNewNote={newNote}
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
    {settings}
    onSettingsSaved={(saved) => (settings = saved)}
    onSaved={() => (hasApiKey = true)}
    onClose={() => (settingsOpen = false)}
  />
{/if}
