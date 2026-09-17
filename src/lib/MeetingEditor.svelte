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

  export function dirtyAiNotesDraft(
    editing: boolean,
    draft: string,
    savedNotes: string
  ): string | undefined {
    return editing && draft !== savedNotes ? draft : undefined;
  }

  export async function flushMeetingDrafts(
    flushOriginal: () => Promise<void>,
    readAiDraft: () => string | undefined,
    saveAiDraft: (draft: string) => Promise<void>
  ): Promise<void> {
    await flushOriginal();
    const aiDraft = readAiDraft();
    if (aiDraft !== undefined) await saveAiDraft(aiDraft);
  }

  export type SuggestionKey = 'title' | 'context' | 'category' | 'participants';
  export type MeetingMetadata = { title: string; context: string; folder: string; attendees: string };

  export function metadataAfterSuggestion(
    current: MeetingMetadata,
    key: SuggestionKey,
    updated: MeetingMetadata
  ): MeetingMetadata {
    if (key === 'title') return { ...current, title: updated.title };
    if (key === 'context') return { ...current, context: updated.context };
    if (key === 'category') return { ...current, folder: updated.folder };
    return { ...current, attendees: updated.attendees };
  }
</script>

<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    askMeeting,
    applySuggestion,
    createCloseHandler,
    destroyCurrentWindow,
    exportMarkdown,
    onWindowCloseRequested,
    refreshInsights,
    saveAiNotes,
    saveSession
  } from './api';
  import { createAutosave } from './autosave';
  import { parseMeetingMarkdown } from './markdown';
  import InlineMarkdown from './InlineMarkdown.svelte';
  import { aiNotes, meetingMarkdown, meetingViewLabel } from './meeting-workspace';
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
  let editingAiNotes = $state(false);
  let aiNotesDraft = $state('');
  let aiNotesSaving = $state(false);
  let aiNotesSave: Promise<void> | null = null;
  let aiNotesError = $state('');
  let suggestionPending = $state('');
  let suggestionError = $state('');
  let refreshing = $state(false);
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
    await flushMeetingDrafts(async () => {
      await autosave.flush();
      if (aiNotesSave) await aiNotesSave;
    },
      () => dirtyAiNotesDraft(editingAiNotes, aiNotesDraft, aiNotes(session) ?? ''), saveAiNotesValue);
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

  function beginAiNoteEdit() {
    aiNotesDraft = aiNotes(session) ?? '';
    aiNotesError = '';
    editingAiNotes = true;
  }

  function saveAiNotesValue(notes: string | null): Promise<void> {
    if (aiNotesSave) return aiNotesSave;
    aiNotesSaving = true;
    aiNotesError = '';
    aiNotesSave = (async () => {
      try {
        const updated = await saveAiNotes(session.id, notes);
        onSessionChange(updated);
        aiNotesDraft = aiNotes(updated) ?? '';
        editingAiNotes = false;
      } catch (error) {
        aiNotesError = errorMessage(error, 'AI notes could not be saved. Your typed notes were kept.');
        throw error;
      } finally {
        aiNotesSaving = false;
        aiNotesSave = null;
      }
    })();
    return aiNotesSave;
  }

  async function persistAiNotes(notes: string | null) {
    aiNotesSaving = true;
    try {
      await autosave.flush();
      await saveAiNotesValue(notes);
    } catch {
      // The editor stays open with its draft and the inline error offers another save attempt.
    } finally {
      aiNotesSaving = false;
    }
  }

  async function changeSuggestion(key: SuggestionKey, action: 'apply' | 'dismiss') {
    suggestionPending = `${key}:${action}`;
    suggestionError = '';
    try {
      await flush();
      const updated = await applySuggestion(session.id, key, action);
      onSessionChange(updated);
      if (action === 'apply') {
        ({ title, context, folder, attendees } = metadataAfterSuggestion(
          { title, context, folder, attendees },
          key,
          { title: updated.title, context: updated.context, folder: updated.folder, attendees: updated.attendees.join(', ') }
        ));
      }
    } catch (error) {
      suggestionError = errorMessage(error, 'The suggestion could not be changed. Try again.');
    } finally {
      suggestionPending = '';
    }
  }

  async function refreshMeetingInsights() {
    actionMenu.hidePopover();
    refreshing = true;
    menuExportError = false;
    menuExportStatus = '';
    try {
      await flush();
      onSessionChange(await refreshInsights(session.id));
      menuExportStatus = 'AI notes and suggestions refreshed from saved text.';
    } catch (error) {
      menuExportError = true;
      menuExportStatus = errorMessage(error, 'AI notes could not be refreshed. Your saved text was kept.');
    } finally {
      refreshing = false;
    }
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
      {#if session.status === 'complete' && (session.transcript || session.originalNotes || session.enrichedNotes)}
        <button type="button" disabled={refreshing} onclick={refreshMeetingInsights}>{refreshing ? 'Refreshing…' : 'Refresh AI notes'}</button>
      {/if}
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
      rows="1"
      disabled={Boolean(suggestionPending)}
    ></textarea>
    {#if session.aiSuggestions?.title && !(session.dismissedSuggestions ?? []).includes('title')}
      <div class="field-suggestion title-suggestion">
        <span>Suggested title: <strong>{session.aiSuggestions.title.value}</strong></span>
        <div><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('title', 'apply')}>Use</button><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('title', 'dismiss')}>Dismiss</button></div>
      </div>
    {/if}

    <div class="document-fields">
      <label>
        <span>Participants</span>
        <span class="field-control"><input value={attendees} oninput={updateAttendees} placeholder="Add names, separated by commas" disabled={Boolean(suggestionPending)} />
          {#if session.aiSuggestions?.participants.length && !(session.dismissedSuggestions ?? []).includes('participants')}
            <span class="field-suggestion"><span>Suggested: <strong>{session.aiSuggestions.participants.map((item) => item.name).join(', ')}</strong></span><span><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('participants', 'apply')}>Add</button><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('participants', 'dismiss')}>Dismiss</button></span></span>
          {/if}
        </span>
      </label>
      <label>
        <span>Context</span>
        <span class="field-control"><textarea value={context} oninput={updateContext} rows="1" placeholder="What should this meeting accomplish?" disabled={Boolean(suggestionPending)}></textarea>
          {#if session.aiSuggestions?.context && !(session.dismissedSuggestions ?? []).includes('context')}
            <span class="field-suggestion"><span>Suggested: <strong>{session.aiSuggestions.context.value}</strong></span><span><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('context', 'apply')}>Use</button><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('context', 'dismiss')}>Dismiss</button></span></span>
          {/if}
        </span>
      </label>
      <label>
        <span>Category</span>
        <span class="field-control"><input value={folder} oninput={updateFolder} placeholder="Acquisitions, leasing, or another project" disabled={Boolean(suggestionPending)} />
          {#if session.aiSuggestions?.category && !(session.dismissedSuggestions ?? []).includes('category')}
            <span class="field-suggestion"><span>Suggested: <strong>{session.aiSuggestions.category.value}</strong></span><span><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('category', 'apply')}>Use</button><button type="button" disabled={Boolean(suggestionPending)} onclick={() => changeSuggestion('category', 'dismiss')}>Dismiss</button></span></span>
          {/if}
        </span>
      </label>
    </div>
    {#if suggestionError}<p class="suggestion-error" role="alert">{suggestionError}</p>{/if}
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
    <details class="capture-disclosure">
      <summary>Details and notices{session.warnings?.length ? ` · ${session.warnings.length} ${session.warnings.length === 1 ? 'notice' : 'notices'}` : ''}</summary>
      {#if session.captureHealth}<p>{captureCoverage(session.captureHealth)}</p>{/if}
      {#each session.warnings ?? [] as warning}<p>{warning}</p>{/each}
    </details>
  {/if}

  <section class="notes-section" aria-labelledby="notes-title">
    <div class="notes-heading-row">
      <div><h2 id="notes-title">{meetingViewLabel(view)}</h2><p class="view-description">{view === 'original' ? 'Notes you wrote yourself.' : view === 'enhanced' ? 'Editable notes generated from saved meeting text.' : 'Saved words organized for reading.'}</p></div>
      {#if session.status === 'complete' || session.transcript !== null}
        <div class="result-switch" aria-label="Note version">
          <button
            type="button"
            class:active={view === 'original'}
            aria-pressed={view === 'original'}
            onclick={() => (view = 'original')}
          >Your notes</button>
          {#if session.status === 'complete'}
            <button
              type="button"
              class:active={view === 'enhanced'}
              aria-pressed={view === 'enhanced'}
              onclick={() => (view = 'enhanced')}
            >AI notes</button>
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
      <TranscriptView {session} onExport={exportCurrentMeeting} />
    {:else if view === 'enhanced'}
      <div class="ai-notes-toolbar">
        {#if !editingAiNotes}<button type="button" disabled={aiNotesSaving} onclick={beginAiNoteEdit}>Edit AI notes</button>{/if}
        {#if session.editedEnrichedNotes !== null && session.editedEnrichedNotes !== undefined && !editingAiNotes}<button type="button" disabled={aiNotesSaving} onclick={() => persistAiNotes(null)}>Restore generated</button>{/if}
      </div>
      {#if editingAiNotes}
        <div class="ai-notes-editor"><textarea bind:value={aiNotesDraft} rows="12" dir="auto" disabled={aiNotesSaving}></textarea><div><button type="button" disabled={aiNotesSaving} onclick={() => (editingAiNotes = false)}>Cancel</button><button class="primary" type="button" disabled={aiNotesSaving} onclick={() => persistAiNotes(aiNotesDraft)}>{aiNotesSaving ? 'Saving…' : 'Save AI notes'}</button></div></div>
      {:else if aiNotes(session)}
      <div class="enhanced-notes" dir="auto">
        {#each parseMeetingMarkdown(aiNotes(session) ?? '') as block}
          {#if block.kind === 'heading'}
            <h3><InlineMarkdown text={block.text} /></h3>
          {:else if block.kind === 'bullet'}
            <p class="enhanced-bullet"><span aria-hidden="true">•</span><InlineMarkdown text={block.text} /></p>
          {:else}
            <p><InlineMarkdown text={block.text} /></p>
          {/if}
        {/each}
      </div>
      {:else}
        <div class="notes-empty"><p>No AI notes are saved for this meeting.</p>{#if session.status === 'complete'}<button type="button" onclick={refreshMeetingInsights}>Create from saved text</button>{/if}</div>
      {/if}
      {#if aiNotesError}<p class="deletion-error" role="alert">{aiNotesError}</p>{/if}
    {:else}
      <label class="sr-only" for="original-notes">Original meeting notes</label>
      <textarea
        class="notes-editor"
        class:empty={!originalNotes}
        id="original-notes"
        value={originalNotes}
        oninput={updateNotes}
        onfocus={markOriginalUse}
        placeholder={originalNotes ? '' : ['complete', 'failed'].includes(session.status) ? 'You didn’t type notes during this meeting. Add anything you want to keep.' : 'Start with the questions, numbers, and decisions you want to remember.'}
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
        The transcript and AI notes will be permanently removed. Your notes and any retained audio will stay.
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

  .field-suggestion {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    padding: 7px 0 2px;
    color: var(--muted-text);
    font-size: 11px;
    line-height: 1.4;
  }

  .field-suggestion strong { color: var(--ink); font-weight: 650; }
  .field-suggestion > span:last-child,
  .field-suggestion > div { display: inline-flex; flex: 0 0 auto; gap: 5px; }

  .field-suggestion button,
  .ai-notes-toolbar button,
  .ai-notes-editor button,
  .notes-empty button {
    padding: 4px 7px;
    border: 0;
    border-radius: 6px;
    color: var(--accent-text);
    background: var(--accent-soft);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }

  .field-suggestion button:last-child { color: var(--muted-text); background: transparent; }
  .field-suggestion button:disabled { cursor: wait; opacity: .55; }
  .title-suggestion { max-width: 720px; padding-top: 9px; }
  .suggestion-error { margin: 8px 0 0 82px; color: var(--danger); font-size: 12px; }

  .capture-disclosure {
    max-width: 720px;
    margin-top: 18px;
    color: var(--muted-text);
    font-size: 11px;
    line-height: 1.5;
  }

  .capture-disclosure summary { width: fit-content; cursor: pointer; font-weight: 650; }
  .capture-disclosure p { margin: 7px 0 0; }

  .ai-notes-toolbar { display: flex; justify-content: flex-end; gap: 6px; min-height: 27px; margin-bottom: 7px; }
  .ai-notes-toolbar button { color: var(--muted-text); background: transparent; }
  .ai-notes-editor { display: grid; gap: 9px; }
  .ai-notes-editor textarea { width: 100%; min-height: 260px; padding: 13px; border: 1px solid var(--line); border-radius: 10px; resize: vertical; color: var(--ink); background: color-mix(in srgb, var(--paper) 76%, var(--sidebar)); font: inherit; font-size: 15px; line-height: 1.65; }
  .ai-notes-editor > div { display: flex; justify-content: flex-end; gap: 7px; }
  .ai-notes-editor button.primary { color: #fff; background: var(--accent-text); }
  .notes-empty { padding: 30px 0 50px; color: var(--muted-text); }
  .notes-empty p { margin: 0 0 12px; }

  @media (max-width: 680px) {
    .field-suggestion { align-items: flex-start; flex-direction: column; gap: 5px; }
    .suggestion-error { margin-left: 0; }
  }
</style>
