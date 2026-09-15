<script module lang="ts">
  import type { SessionStatus } from './types';

  export type MeetingView = 'original' | 'enhanced';

  export function nextMeetingView(
    current: MeetingView,
    previousStatus: SessionStatus,
    nextStatus: SessionStatus,
    originalUsedWhileProcessing: boolean
  ): MeetingView {
    return previousStatus === 'processing' && nextStatus === 'complete' && !originalUsedWhileProcessing
      ? 'enhanced'
      : current;
  }
</script>

<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    createCloseHandler,
    destroyCurrentWindow,
    onWindowCloseRequested,
    saveSession
  } from './api';
  import { createAutosave } from './autosave';
  import { parseMeetingMarkdown } from './markdown';
  import RecordingDock from './RecordingDock.svelte';
  import type { Session, UpdateSessionInput } from './types';

  let {
    session,
    hasApiKey,
    recordingStartedAt,
    onRecordingStarted,
    onSessionChange,
    onOpenSettings,
    onDeleteTranscript,
    onDeleteMeeting
  }: {
    session: Session;
    hasApiKey: boolean;
    recordingStartedAt: number | null;
    onRecordingStarted: (id: string, baseline: number) => void;
    onSessionChange: (session: Session) => void;
    onOpenSettings: () => void;
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
    if (session.status !== 'processing') originalUsedWhileProcessing = false;
    lastStatus = session.status;
  });

  onMount(() =>
    onWindowCloseRequested(createCloseHandler(flush, destroyCurrentWindow))
  );

  onDestroy(() => void autosave.flush().catch(() => {}));

  function errorMessage(error: unknown, fallback: string) {
    if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
      return error.message;
    }
    return error instanceof Error ? error.message : fallback;
  }

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
      <button
        type="button"
        disabled={session.transcript === null || session.status === 'recording' || session.status === 'processing'}
        onclick={() => requestDeletion('transcript')}
      >Delete transcript</button>
      <button
        type="button"
        disabled={session.status === 'recording' || session.status === 'processing'}
        onclick={() => requestDeletion('meeting')}
      >Delete meeting</button>
    </div>
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

  <section class="notes-section" aria-labelledby="notes-title">
    <div class="notes-heading-row">
      <h2 id="notes-title">Notes</h2>
      {#if session.status === 'complete'}
        <div class="result-switch" aria-label="Note version">
          <button
            type="button"
            class:active={view === 'original'}
            aria-pressed={view === 'original'}
            onclick={() => (view = 'original')}
          >Original</button>
          <button
            type="button"
            class:active={view === 'enhanced'}
            aria-pressed={view === 'enhanced'}
            onclick={() => (view = 'enhanced')}
          >Enhanced</button>
        </div>
      {/if}
    </div>

    {#if view === 'enhanced' && session.enrichedNotes}
      <div class="enhanced-notes">
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
      ></textarea>
    {/if}

    <p class:error={saveStatus && !['Unsaved', 'Saving…', 'Saved'].includes(saveStatus)} class="save-status" aria-live="polite">
      {saveStatus}
    </p>
  </section>

  <RecordingDock
    {session}
    {hasApiKey}
    {recordingStartedAt}
    {onRecordingStarted}
    onFlush={flush}
    {onSessionChange}
    {onOpenSettings}
  />
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
    color: var(--danger);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .meeting-actions button:hover { background: var(--sidebar); }
  .meeting-actions button:disabled { color: var(--muted-text); cursor: not-allowed; opacity: 0.55; }

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
</style>
