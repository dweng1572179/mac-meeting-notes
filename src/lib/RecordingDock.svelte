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

  export function captureErrorMessage(error: unknown) {
    const detail =
      error && typeof error === 'object' && 'message' in error && typeof error.message === 'string'
        ? error.message
        : error instanceof Error
          ? error.message
          : 'The action could not be completed.';
    const code = error && typeof error === 'object' && 'code' in error ? error.code : '';
    if (code === 'microphone_permission') {
      return `${detail} Open System Settings → Privacy & Security → Microphone, enable Meeting Notes, then try again.`;
    }
    return code === 'audio_permission'
      ? `${detail} Open System Settings → Privacy & Security → Screen & System Audio Recording, enable Meeting Notes, then try again.`
      : detail;
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { retryProcessing, startRecording, stopRecording } from './api';
  import type { RecordingHealth, Session } from './types';

  let {
    session,
    hasApiKey,
    health = null,
    recordingStartedAt,
    onRecordingStarted,
    onFlush,
    onSessionChange,
    onOpenSettings
  }: {
    session: Session;
    hasApiKey: boolean;
    health?: RecordingHealth | null;
    recordingStartedAt: number | null;
    onRecordingStarted: (id: string, baseline: number) => void;
    onFlush: () => Promise<void>;
    onSessionChange: (session: Session) => void;
    onOpenSettings: () => void;
  } = $props();

  let pending = $state<'start' | 'stop' | 'retry' | null>(null);
  let actionError = $state('');
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
      onSessionChange({ ...session, status: 'recording', error: null });
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
    pending = 'retry';
    actionError = '';
    try {
      await onFlush();
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
    <span>{health?.warnings.length ? 'Check audio capture' : 'Recording'}</span>
    <time aria-label={`Elapsed recording time ${elapsedLabel()}`}>{elapsedLabel()}</time>
    <button type="button" disabled={pending !== null} onclick={stop}>
      {pending === 'stop' ? 'Stopping…' : 'Stop'}
    </button>
  </div>
{:else if session.status === 'processing'}
  <div class="recording-dock processing" role="status" aria-live="polite">
    <span class="processing-dot" aria-hidden="true"></span>
    <span>Processing meeting{session.transcription?.length ? ` · ${session.transcription.flatMap((track) => track.chunks).filter((chunk) => chunk.transcript !== null).length}/${session.transcription.flatMap((track) => track.chunks).length} chunks saved` : ''}</span>
    <span class="processing-line" aria-hidden="true"></span>
  </div>
{:else if session.status === 'failed' && (session.audioPath !== null || session.microphoneAudioPath !== null || session.transcript !== null)}
  <div class="failure-dock" role="alert">
    <p>{session.error?.message ?? 'Processing could not finish.'}</p>
    <button type="button" disabled={pending !== null} onclick={retry}>
      {pending === 'retry' ? 'Retrying…' : 'Retry'}
    </button>
  </div>
{:else if session.status === 'draft' || session.status === 'failed'}
  <div class="recording-dock draft">
    {#if session.status === 'failed'}
      <span class="failed-start-message" role="alert">{session.error?.message ?? 'Recording could not start.'}</span>
    {/if}
    <button class="start-meeting" type="button" disabled={pending !== null} onclick={start}>
      <span aria-hidden="true"></span>
      {pending === 'start' ? 'Starting…' : 'Start meeting'}
    </button>
  </div>
{/if}

{#if actionError}
  <p class="dock-error" role="alert">{actionError}</p>
{/if}
