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
    onOpenSettings
  }: {
    session: Session;
    hasApiKey: boolean;
    recordingStartedAt: number | null;
    onRecordingStarted: (id: string, baseline: number) => void;
    onSessionChange: (session: Session) => void;
    onOpenSettings: () => void;
  } = $props();

  const initial = <T,>(read: () => T) => read();
  let title = $state(initial(() => session.title));
  let context = $state(initial(() => session.context));
  let attendees = $state(initial(() => session.attendees.join(', ')));
  let originalNotes = $state(initial(() => session.originalNotes));
  let view = $state<MeetingView>(
    initial(() => (session.status === 'complete' ? 'enhanced' : 'original'))
  );
  let saveStatus = $state('');
  let lastStatus = initial(() => session.status);
  let originalUsedWhileProcessing = false;

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

  onMount(() => {
    let closing = false;
    return onWindowCloseRequested(async () => {
      if (closing) return;
      closing = true;
      try {
        await flush();
        await destroyCurrentWindow();
      } catch {
        closing = false;
      }
    });
  });

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

  const meetingDate = (startedAt: string) =>
    new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(
      new Date(startedAt)
    );
</script>

<article class="meeting-document" aria-labelledby="meeting-heading">
  <header class="document-header">
    <h1 class="sr-only" id="meeting-heading">{title || 'Untitled meeting'}</h1>
    <time class="meeting-date" datetime={session.startedAt}>{meetingDate(session.startedAt)}</time>
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
