<script lang="ts">
  import { onMount } from 'svelte';
  import { retryProcessing, startRecording, stopRecording } from './api';
  import type { Session } from './types';

  let {
    session,
    hasApiKey,
    onFlush,
    onSessionChange,
    onOpenSettings
  }: {
    session: Session;
    hasApiKey: boolean;
    onFlush: () => Promise<void>;
    onSessionChange: (session: Session) => void;
    onOpenSettings: () => void;
  } = $props();

  let pending = $state<'start' | 'stop' | 'retry' | null>(null);
  let actionError = $state('');
  let now = $state(0);
  let recordingStarted = $state<number | null>(null);

  $effect(() => {
    if (session.status === 'recording' && recordingStarted === null) {
      recordingStarted = performance.now();
      now = recordingStarted;
    } else if (session.status !== 'recording') {
      recordingStarted = null;
    }
  });

  onMount(() => {
    const timer = window.setInterval(() => {
      if (recordingStarted !== null) now = performance.now();
    }, 250);
    return () => window.clearInterval(timer);
  });

  const elapsed = () => Math.max(0, Math.floor((now - (recordingStarted ?? now)) / 1000));
  const elapsedLabel = () => {
    const seconds = elapsed();
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainder = seconds % 60;
    return hours
      ? `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`
      : `${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
  };

  function message(error: unknown) {
    const detail =
      error && typeof error === 'object' && 'message' in error && typeof error.message === 'string'
        ? error.message
        : error instanceof Error
          ? error.message
          : 'The action could not be completed.';
    const code = error && typeof error === 'object' && 'code' in error ? error.code : '';
    return code === 'audio_capture' || detail.includes('OSStatus')
      ? `${detail} Open System Settings → Privacy & Security → Screen & System Audio Recording, enable Meeting Notes, then try again.`
      : detail;
  }

  async function start() {
    pending = 'start';
    actionError = '';
    try {
      await onFlush();
      if (!hasApiKey) {
        onOpenSettings();
        return;
      }
      await startRecording(session.id);
      recordingStarted = performance.now();
      now = recordingStarted;
      onSessionChange({ ...session, status: 'recording', error: null });
    } catch (error) {
      actionError = message(error);
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
      actionError = message(error);
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
      actionError = message(error);
    } finally {
      pending = null;
    }
  }
</script>

{#if session.status === 'recording'}
  <div class="recording-dock live" role="status" aria-label="Meeting recording in progress">
    <span class="recording-dot" aria-hidden="true"></span>
    <span>Recording</span>
    <time>{elapsedLabel()}</time>
    <button type="button" disabled={pending !== null} onclick={stop}>
      {pending === 'stop' ? 'Stopping…' : 'Stop'}
    </button>
  </div>
{:else if session.status === 'processing'}
  <div class="recording-dock processing" role="status" aria-live="polite">
    <span class="processing-dot" aria-hidden="true"></span>
    <span>Processing meeting</span>
    <span class="processing-line" aria-hidden="true"></span>
  </div>
{:else if session.status === 'failed' && (session.audioPath !== null || session.transcript !== null)}
  <div class="failure-dock" role="alert">
    <p>{session.error?.message ?? 'Processing could not finish.'}</p>
    <button type="button" disabled={pending !== null} onclick={retry}>
      {pending === 'retry' ? 'Retrying…' : 'Retry'}
    </button>
  </div>
{:else if session.status === 'draft' || session.status === 'failed'}
  <div class="recording-dock draft">
    <button class="start-meeting" type="button" disabled={pending !== null} onclick={start}>
      <span aria-hidden="true"></span>
      {pending === 'start' ? 'Starting…' : 'Start meeting'}
    </button>
  </div>
{/if}

{#if actionError}
  <p class="dock-error" role="alert">{actionError}</p>
{/if}
