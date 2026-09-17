<script module lang="ts">
  import type { SessionStatus } from './types';

  export type MeetingView = 'original' | 'enhanced' | 'transcript';

  export function nextMeetingView(
    current: MeetingView,
    previousStatus: SessionStatus,
    nextStatus: SessionStatus,
    originalUsedWhileProcessing: boolean
  ): MeetingView {
    return current !== 'transcript' && previousStatus === 'processing' && nextStatus === 'complete' && !originalUsedWhileProcessing
      ? 'enhanced'
      : current;
  }
</script>

<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    askMeeting,
    createCloseHandler,
    destroyCurrentWindow,
    exportMarkdown,
    onWindowCloseRequested,
    saveSession
  } from './api';
  import { createAutosave } from './autosave';
  import { parseMeetingMarkdown } from './markdown';
  import { meetingMarkdown } from './meeting-workspace';
  import MeetingQuestion from './MeetingQuestion.svelte';
  import { errorMessage } from './recovery';
  import RecordingDock, { captureCoverage } from './RecordingDock.svelte';
  import TranscriptView from './TranscriptView.svelte';
  import type { RecordingHealth, Session, UpdateSessionInput } from './types';

  let {
    session,
    hasApiKey,
    health = null,
    recordingStartedAt,
    onRecordingStarted,
    onSessionChange,
    onOpenSettings,
    onNewNote,
    onDeleteTranscript,
    onDeleteMeeting
  }: {
    session: Session;
    hasApiKey: boolean;
    health?: RecordingHealth | null;
    recordingStartedAt: number | null;
    onRecordingStarted: (id: string, baseline: number) => void;
    onSessionChange: (session: Session) => void;
    onOpenSettings: () => void;
    onNewNote: () => void;
    onDeleteTranscript: (id: string) => Promise<void>;
    onDeleteMeeting: (id: string) => Promise<void>;
  } = $props();

  const initial = <T,>(read: () => T) => read();
  let title = $state(initial(() => session.title));
  let context = $state(initial(() => session.context));
  let attendees = $state(initial(() => session.attendees.join(', ')));
  let folder = $state(initial(() => session.folder));
  let originalNotes = $state(initial(() => session.originalNotes));
  let view = $state<MeetingView>(
    initial(() => (session.status === 'complete' ? 'enhanced' : 'original'))
  );
  let saveStatus = $state('');
  let lastStatus = initial(() => session.status);
  let originalUsedWhileProcessing = false;
  let confirmation = $state<'transcript' | 'meeting' | null>(null);
  let deleting = $state(false);
  let deleteError = $state('');
  let menuExporting = $state(false);
  let menuExportStatus = $state('');
  let menuExportError = $state(false);
  let actionMenu: HTMLDivElement;
  let confirmationDialog: HTMLDialogElement;

  const autosave = createAutosave<UpdateSessionInput>(450, async (input) => {
    saveStatus = 'Saving…';
    try {
      onSessionChange(await saveSession(input));
      saveStatus = 'Saved';
    } catch (error) {
      saveStatus = errorMessage(error, 'Changes could not be saved. Keep this window open and try editing again.');
      throw error;
    }
  });

  $effect(() => {
    if (session.status === 'processing' && lastStatus !== 'processing') {
      originalUsedWhileProcessing = document.activeElement?.id === 'original-notes';
    }
    view = nextMeetingView(view, lastStatus, session.status, originalUsedWhileProcessing);
    if (view === 'transcript' && session.transcript === null) view = 'original';
    if (session.status !== 'processing') originalUsedWhileProcessing = false;
    lastStatus = session.status;
  });

  onMount(() =>
    onWindowCloseRequested(createCloseHandler(flush, destroyCurrentWindow))
  );

  onDestroy(() => void autosave.flush().catch(() => {}));

  function input() {
    return {
      id: session.id,
      title,
      context,
      attendees: attendees.split(',').map((name) => name.trim()).filter(Boolean),
      folder,
      originalNotes
    };
  }

  function scheduleSave() {
    saveStatus = 'Unsaved';
    autosave.schedule(input());
  }

  function updateTitle(event: Event) {
    title = (event.currentTarget as HTMLTextAreaElement).value;
    scheduleSave();
  }

  function updateContext(event: Event) {
    context = (event.currentTarget as HTMLTextAreaElement).value;
    scheduleSave();
  }

  function updateAttendees(event: Event) {
    attendees = (event.currentTarget as HTMLInputElement).value;
    scheduleSave();
  }

  function updateFolder(event: Event) {
    folder = (event.currentTarget as HTMLInputElement).value;
    scheduleSave();
  }

  function updateNotes(event: Event) {
    markOriginalUse();
    originalNotes = (event.currentTarget as HTMLTextAreaElement).value;
    scheduleSave();
  }

  function markOriginalUse() {
    if (session.status === 'processing') originalUsedWhileProcessing = true;
  }

  export async function flush() {
    await autosave.flush();
  }

  function currentSession(): Session {
    return {
      ...session,
      title,
      context,
      attendees: input().attendees,
      folder,
      originalNotes
    };
  }

  async function exportCurrentMeeting() {
    await flush();
    return exportMarkdown(title || 'Untitled meeting', meetingMarkdown(currentSession()));
  }

  async function askCurrentMeeting(question: string) {
    await flush();
    return askMeeting(session.id, question);
  }

  async function exportFromMenu() {
    actionMenu.hidePopover();
    menuExporting = true;
    menuExportStatus = '';
    menuExportError = false;
    try {
      menuExportStatus = `Saved to ${await exportCurrentMeeting()}`;
    } catch (error) {
      menuExportError = true;
      menuExportStatus = errorMessage(error, 'The Markdown copy could not be saved. Try again.');
    } finally {
      menuExporting = false;
    }
  }

  function requestDeletion(target: 'transcript' | 'meeting') {
    actionMenu.hidePopover();
    confirmation = target;
    deleteError = '';
    confirmationDialog.showModal();
  }

  async function confirmDeletion() {
    if (!confirmation) return;
    deleting = true;
    deleteError = '';
    try {
      await flush();
      if (confirmation === 'transcript') await onDeleteTranscript(session.id);
      else await onDeleteMeeting(session.id);
      confirmationDialog.close();
    } catch (error) {
      deleteError = errorMessage(error, 'This meeting could not be changed. Try again.');
    } finally {
      deleting = false;
    }
  }

  const meetingDate = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(
      new Date(startedAt)
    );
</script>

<article class="meeting-document" aria-labelledby="meeting-heading">
  <header class="document-header">
    <h1 class="sr-only" id="meeting-heading">{title || 'Untitled meeting'}</h1>
    <div class="document-toolbar">
      <time class="meeting-date" datetime={session.startedAt}>{meetingDate(session.startedAt)}</time>
      <button
        class="meeting-menu-trigger"
        type="button"
        aria-label={`Actions for ${title || 'Untitled meeting'}`}
        popovertarget="meeting-actions"
      >•••</button>
    </div>
    <div bind:this={actionMenu} class="meeting-actions" id="meeting-actions" popover="auto">
      <button type="button" disabled={menuExporting} onclick={exportFromMenu}>
        {menuExporting ? 'Saving…' : 'Export Markdown'}
      </button>
      <button
        class="danger-action"
        type="button"
        disabled={session.transcript === null || session.status === 'recording' || session.status === 'processing'}
        onclick={() => requestDeletion('transcript')}
      >Delete transcript</button>
      <button
        class="danger-action"
        type="button"
        disabled={session.status === 'recording' || session.status === 'processing'}
        onclick={() => requestDeletion('meeting')}
      >Delete meeting</button>
    </div>
    {#if menuExportStatus}
      <p class:error={menuExportError} class="menu-export-status" aria-live="polite">{menuExportStatus}</p>
    {/if}
    <label class="sr-only" id="meeting-title-label" for="meeting-title">Title</label>
    <textarea
      class="meeting-title-input"
      id="meeting-title"
      value={title}
      oninput={updateTitle}
      aria-labelledby="meeting-title-label"
      rows="2"
    ></textarea>

    <div class="document-fields">
      <label>
        <span>Attendees</span>
        <input
          value={attendees}
          oninput={updateAttendees}
          placeholder="Add names, separated by commas"
        />
      </label>
      <label>
        <span>Context</span>
        <textarea
          value={context}
          oninput={updateContext}
          rows="2"
          placeholder="What should this meeting accomplish?"
        ></textarea>
      </label>
      <label>
        <span>Folder</span>
        <input
          value={folder}
          oninput={updateFolder}
          placeholder="Acquisitions, leasing, or another project"
        />
      </label>
    </div>
  </header>

  <RecordingDock
    {session}
    {hasApiKey}
    {health}
    {recordingStartedAt}
    {onRecordingStarted}
    onFlush={flush}
    {onSessionChange}
    {onOpenSettings}
    {onNewNote}
  />

  {#if session.captureHealth || session.warnings?.length}
    <aside class="capture-health" aria-label="Saved recording coverage">
      {#if session.captureHealth}<p>{captureCoverage(session.captureHealth)}</p>{/if}
      {#each session.warnings ?? [] as warning}<p>{warning}</p>{/each}
    </aside>
  {/if}

  <section class="notes-section" aria-labelledby="notes-title">
    <div class="notes-heading-row">
      <h2 id="notes-title">Notes</h2>
      {#if session.status === 'complete' || session.transcript !== null}
        <div class="result-switch" aria-label="Note version">
          <button
            type="button"
            class:active={view === 'original'}
            aria-pressed={view === 'original'}
            onclick={() => (view = 'original')}
          >Original</button>
          {#if session.status === 'complete'}
            <button
              type="button"
              class:active={view === 'enhanced'}
              aria-pressed={view === 'enhanced'}
              onclick={() => (view = 'enhanced')}
            >Enhanced</button>
          {/if}
          {#if session.transcript !== null}
            <button
              type="button"
              class:active={view === 'transcript'}
              aria-pressed={view === 'transcript'}
              onclick={() => (view = 'transcript')}
            >Transcript</button>
          {/if}
        </div>
      {/if}
    </div>

    {#if view === 'transcript' && session.transcript !== null}
      <TranscriptView
        transcript={session.transcript}
        partial={session.status !== 'complete'}
        onExport={exportCurrentMeeting}
      />
    {:else if view === 'enhanced' && session.enrichedNotes}
      <div class="enhanced-notes" dir="auto">
        {#each parseMeetingMarkdown(session.enrichedNotes) as block}
          {#if block.kind === 'heading'}
            <h3>{block.text}</h3>
          {:else if block.kind === 'bullet'}
            <p class="enhanced-bullet"><span aria-hidden="true">•</span>{block.text}</p>
          {:else}
            <p>{block.text}</p>
          {/if}
        {/each}
      </div>
    {:else}
      <label class="sr-only" for="original-notes">Original meeting notes</label>
      <textarea
        class="notes-editor"
        id="original-notes"
        value={originalNotes}
        oninput={updateNotes}
        onfocus={markOriginalUse}
        placeholder="Start with the questions, numbers, and decisions you want to remember."
        spellcheck="true"
        dir="auto"
      ></textarea>
    {/if}

    <div class="save-row">
      <p class:error={saveStatus && !['Unsaved', 'Saving…', 'Saved'].includes(saveStatus)} class="save-status" aria-live="polite">
        {saveStatus}
      </p>
      {#if saveStatus && !['Unsaved', 'Saving…', 'Saved'].includes(saveStatus)}
        <button class="retry-save" type="button" onclick={() => void flush().catch(() => {})}>Retry save</button>
      {/if}
    </div>
  </section>

  {#if session.status === 'complete' && session.transcript !== null}
    <MeetingQuestion {hasApiKey} onAsk={askCurrentMeeting} {onOpenSettings} />
  {/if}


</article>

<dialog
  bind:this={confirmationDialog}
  class="settings-dialog deletion-dialog"
  aria-labelledby="deletion-title"
  oncancel={(event) => { if (deleting) event.preventDefault(); }}
  onclose={() => { confirmation = null; deleteError = ''; }}
>
  {#if confirmation}
    <h2 id="deletion-title">
      Delete {confirmation === 'transcript' ? 'transcript' : 'meeting'} for “{title || 'Untitled meeting'}”?
    </h2>
    <p class="settings-copy">
      {#if confirmation === 'transcript'}
        The transcript and enhanced notes will be permanently removed. Your original notes and any retained audio will stay.
      {:else if session.audioPath !== null || session.microphoneAudioPath !== null}
        This meeting, its notes, and its retained audio recording will be permanently removed.
      {:else}
        This meeting and its notes will be permanently removed. There is no retained audio recording to remove.
      {/if}
    </p>
    {#if deleteError}<p class="deletion-error" role="alert">{deleteError}</p>{/if}
    <div class="deletion-footer">
      <button type="button" disabled={deleting} onclick={() => confirmationDialog.close()}>Cancel</button>
      <button class="delete-button" type="button" disabled={deleting} onclick={confirmDeletion}>
        {deleting ? 'Deleting…' : `Delete ${confirmation}`}
      </button>
    </div>
  {/if}
</dialog>

<style>
  .document-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .meeting-menu-trigger {
    min-width: 34px;
    min-height: 30px;
    padding: 0 8px 5px;
    border: 0;
    border-radius: 7px;
    color: var(--muted-text);
    background: transparent;
    font-size: 17px;
    line-height: 1;
    cursor: pointer;
  }

  .meeting-menu-trigger:hover { background: var(--sidebar); }

  .meeting-actions {
    width: 180px;
    padding: 5px;
    border: 1px solid var(--line);
    border-radius: 9px;
    color: var(--ink);
    background: var(--paper);
    box-shadow: 0 12px 34px rgba(41, 41, 33, 0.18);
  }

  .meeting-actions::backdrop { background: transparent; }

  .meeting-actions button {
    display: block;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 6px;
    color: var(--ink);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .meeting-actions button:hover { background: var(--sidebar); }
  .meeting-actions button:disabled { color: var(--muted-text); cursor: not-allowed; opacity: 0.55; }
  .meeting-actions .danger-action { color: var(--danger); }

  .menu-export-status {
    margin: 8px 0 0;
    color: var(--muted-text);
    font-size: 12px;
    line-height: 1.45;
  }

  .menu-export-status.error { color: var(--danger); }

  .deletion-dialog { width: min(100%, 520px); }
  .deletion-dialog h2 { line-height: 1.2; }

  .deletion-error {
    margin: 0 0 18px;
    color: var(--danger);
    font-size: 13px;
    line-height: 1.5;
  }

  .deletion-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }

  .deletion-footer button {
    min-height: 38px;
    padding: 0 14px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--paper);
    font-weight: 600;
    cursor: pointer;
  }

  .deletion-footer .delete-button {
    border-color: var(--danger);
    color: #fff;
    background: var(--danger);
  }

  .deletion-footer button:disabled { cursor: wait; opacity: 0.65; }

  .save-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .retry-save {
    padding: 5px 9px;
    border: 1px solid var(--line);
    border-radius: 7px;
    color: var(--ink);
    background: var(--paper);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
</style>
