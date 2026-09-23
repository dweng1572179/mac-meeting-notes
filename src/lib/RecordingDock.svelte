<script module lang="ts">
  export function captureCoverage(health: { wallSeconds: number; system: { capturedSeconds: number }; microphone: { capturedSeconds: number } }) {
    const duration = (value: number) => {
      const seconds = Math.max(0, Math.floor(value));
      const tail = `${Math.floor(seconds / 60) % 60}:${String(seconds % 60).padStart(2, '0')}`;
      return seconds >= 3600 ? `${Math.floor(seconds / 3600)}:${tail.padStart(5, '0')}` : tail;
    };
    return `Elapsed ${duration(health.wallSeconds)} · Captured: system ${duration(health.system.capturedSeconds)}, microphone ${duration(health.microphone.capturedSeconds)}`;
  }

  export function elapsedRecordingSeconds(startedAt: number, now: number, nativeWallSeconds = 0) {
    return Math.floor(Math.max(0, (now - startedAt) / 1000, nativeWallSeconds));
  }

  import { errorMessage } from './recovery';
  export const captureErrorMessage = errorMessage;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { retryProcessing, startRecording, stopRecording } from './api';
  import { processingLabel, recordingTranscriptionLabel, recoveryActions } from './recovery';
  import type { RecordingHealth, Session } from './types';

  let {
    session,
    hasApiKey,
    health = null,
    recordingStartedAt,
    onRecordingStarted,
    onFlush,
    onSessionChange,
    onOpenSettings,
    onNewNote
  }: {
    session: Session;
    hasApiKey: boolean;
    health?: RecordingHealth | null;
    recordingStartedAt: number | null;
    onRecordingStarted: (id: string, baseline: number) => void;
    onFlush: () => Promise<void>;
    onSessionChange: (session: Session) => void;
    onOpenSettings: () => void;
    onNewNote: () => void;
  } = $props();

  let pending = $state<'start' | 'stop' | 'retry' | null>(null);
  let actionError = $state('');
  let recovery = $derived(recoveryActions(session));
  let now = $state(0);
  const initial = <T,>(read: () => T) => read();
  let recordingStarted = $state(initial(() => recordingStartedAt));

  $effect(() => {
    if (session.status === 'recording') {
      if (
        recordingStartedAt !== null &&
        (recordingStarted === null || recordingStartedAt < recordingStarted)
      ) {
        recordingStarted = recordingStartedAt;
        now = Math.max(now, performance.now());
      } else if (recordingStarted === null) {
        recordingStarted = performance.now();
        now = recordingStarted;
        onRecordingStarted(session.id, recordingStarted);
      }
    } else {
      recordingStarted = null;
    }
  });

  onMount(() => {
    const timer = window.setInterval(() => {
      if (recordingStarted !== null) now = performance.now();
    }, 250);
    return () => window.clearInterval(timer);
  });

  const elapsed = () => elapsedRecordingSeconds(recordingStarted ?? now, now, health?.wallSeconds);
  const elapsedLabel = () => {
    const seconds = elapsed();
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainder = seconds % 60;
    return hours
      ? `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
      : `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
  };

  async function start() {
    pending = 'start';
    actionError = '';
    try {
      await onFlush();
      if (!hasApiKey) {
        onOpenSettings();
        return;
      }
      const recording = await startRecording(session.id);
      const baseline = performance.now();
      recordingStarted = baseline;
      now = baseline;
      onRecordingStarted(recording.sessionId, baseline);
      onSessionChange({ ...session, status: 'recording', segmentedCapture: true, error: null });
    } catch (error) {
      actionError = captureErrorMessage(error);
    } finally {
      pending = null;
    }
  }

  async function stop() {
    pending = 'stop';
    actionError = '';
    try {
      await onFlush();
      onSessionChange(await stopRecording(session.id));
    } catch (error) {
      actionError = captureErrorMessage(error);
    } finally {
      pending = null;
    }
  }

  async function retry() {
    if (pending) return;
    pending = 'retry';
    actionError = '';
    try {
      await onFlush();
      if (!hasApiKey) { onOpenSettings(); return; }
      onSessionChange(await retryProcessing(session.id));
    } catch (error) {
      actionError = captureErrorMessage(error);
    } finally {
      pending = null;
    }
  }
</script>

{#if session.status === 'recording'}
  <div class="recording-dock live" aria-label="Meeting recording in progress">
    <span class="sr-only" role="status">Recording started.</span>
    <span class="recording-dot" aria-hidden="true"></span>
    <div class="live-copy">
      <span>{health?.warnings.length ? 'Check audio capture' : 'Recording'}</span>
      {#if session.segmentedCapture}<small>{recordingTranscriptionLabel(session)}</small>{/if}
    </div>
    <time aria-label={`Elapsed recording time ${elapsedLabel()}`}>{elapsedLabel()}</time>
    <button type="button" disabled={pending !== null} onclick={stop}>
      {pending === 'stop' ? 'Stopping…' : 'Stop'}
    </button>
  </div>
  {#if session.liveTranscriptionError}
    <p class="live-transcription-warning" role="status">Recording continues. {session.liveTranscriptionError.message} Temporary connection or service errors get up to three automatic retries. If processing stays paused, stop and resume it; your audio is kept.</p>
  {/if}
{:else if session.status === 'processing'}
  <div class="recording-dock processing" role="status" aria-live="polite">
    <span class="processing-dot" aria-hidden="true"></span>
    <span>{processingLabel(session)}</span>
    <span class="processing-line" aria-hidden="true"></span>
  </div>
{:else if session.status === 'failed'}
  <div class="failure-dock" class:no-speech={session.error?.code === 'no_speech'} role={session.error?.code === 'no_speech' ? 'status' : 'alert'}>
    <div class="recovery-copy">
      <p>{captureErrorMessage(session.error, 'Processing could not finish.')}</p>
      <p class="recovery-help">{recovery.guidance}</p>
    </div>
    <div class="recovery-buttons">
      {#if recovery.settings || !hasApiKey}
        <button type="button" disabled={pending !== null} onclick={onOpenSettings}>Open settings</button>
      {/if}
      {#if recovery.retryLabel}
        <button type="button" disabled={pending !== null} onclick={retry}>{pending === 'retry' ? 'Retrying…' : recovery.retryLabel}</button>
      {/if}
      {#if recovery.newMeeting}
        <button type="button" disabled={pending !== null} onclick={onNewNote}>New meeting</button>
      {/if}
    </div>
  </div>
{:else if session.status === 'draft'}
  <div class="recording-readiness">
    <p>Captures your microphone and computer audio, and transcribes during the meeting. Connect your headphones before starting.</p>
    <div class="recording-dock draft">
      <button class="start-meeting" type="button" disabled={pending !== null} onclick={start}>
        <span aria-hidden="true"></span>
        {pending === 'start' ? 'Starting…' : hasApiKey ? 'Start meeting' : 'Add API key to start'}
      </button>
    </div>
  </div>
{/if}

{#if actionError}
  <p class="dock-error" role="alert">{actionError}</p>
{/if}

<style>
  .live-copy { display: grid; gap: 3px; }
  .live-copy small { color: #d4d2c9; font-size: 11px; font-weight: 400; }
  .live-transcription-warning { max-width: 720px; padding: 12px 0; color: var(--muted-text); font-size: 13px; line-height: 1.5; }
  .failure-dock { position: static; transform: none; width: 100%; max-width: 720px; margin: 24px 0; padding: 16px; box-shadow: none; flex-wrap: wrap; align-items: flex-start; }
  .failure-dock.no-speech { color: var(--ink); border-color: var(--line); background: var(--sidebar); }
  .no-speech button { color: var(--ink); background: var(--paper); border: 1px solid var(--line); }
  .no-speech button:last-child { color: white; background: var(--accent-text); border-color: var(--accent-text); }
  .recovery-copy { flex: 1 1 260px; min-width: 0; overflow-wrap: anywhere; }
  .recovery-copy .recovery-help { margin-top: 8px; color: var(--muted-text); font-size: 13px; line-height: 1.5; }
  .recovery-buttons { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .recording-readiness { margin-top: 28px; }
  .recording-readiness > p { max-width: 62ch; color: var(--muted-text); font-size: 13px; line-height: 1.5; }
  .recording-readiness .recording-dock { margin-top: 12px; }
</style>
