import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  Bootstrap,
  CreateSessionInput,
  MeetingAnswer,
  RecordingInfo,
  RecordingHealth,
  Session,
  UpdateSessionInput
} from './types';

const basePreviewSession: Session = {
  id: 'simulation-riverside-logistics',
  title: '[SIMULATION] Riverside Logistics Center',
  startedAt: '2026-09-12T17:30:00.000Z',
  endedAt: '2026-09-12T18:14:00.000Z',
  context: '[SIMULATION] Initial acquisition review for a regional distribution property.',
  attendees: ['Alex Morgan', 'Jordan Lee', 'Sam Rivera'],
  folder: 'Acquisitions',
  originalNotes:
    '[SIMULATION]\n\nConfirm tenant rollover exposure and request the latest roof inspection. Underwrite the south bay at market rent.',
  transcript: null,
  enrichedNotes:
    '## Decision\n\nAdvance to a detailed underwriting review.\n\n## Follow-ups\n\n- Request the current rent roll and roof report\n- Revisit the south-bay rent assumption',
  status: 'complete',
  error: null,
  audioPath: null,
  microphoneAudioPath: null
};

const isNative = () => '__TAURI_INTERNALS__' in window;
let previewSession: Session | undefined;

function previewState(): Session {
  const status = new URLSearchParams(window.location.search).get('state');
  const selectedStatus = ['draft', 'recording', 'processing', 'complete', 'failed'].includes(status ?? '')
    ? (status as Session['status'])
    : 'complete';

  return {
    ...basePreviewSession,
    endedAt: ['processing', 'complete', 'failed'].includes(selectedStatus)
      ? basePreviewSession.endedAt
      : null,
    transcript: ['complete', 'failed'].includes(selectedStatus)
      ? '[SIMULATION] The team agreed to continue underwriting and request the latest property documents.'
      : null,
    enrichedNotes: selectedStatus === 'complete' ? basePreviewSession.enrichedNotes : null,
    status: selectedStatus,
    error:
      selectedStatus === 'failed'
        ? { code: 'openai_request', message: '[SIMULATION] Processing could not reach OpenAI.' }
        : null,
    audioPath: ['recording', 'processing', 'failed'].includes(selectedStatus)
      ? '/tmp/simulation-riverside-logistics.m4a'
      : null,
    microphoneAudioPath: ['recording', 'processing', 'failed'].includes(selectedStatus)
      ? '/tmp/simulation-riverside-logistics-mic.m4a'
      : null
  };
}

const previewDelay = () => new Promise((resolve) => setTimeout(resolve, 350));

export async function bootstrap(): Promise<Bootstrap> {
  if (!isNative()) {
    previewSession = previewState();
    return {
      sessions: new URLSearchParams(window.location.search).has('empty') ? [] : [previewSession],
      hasApiKey: new URLSearchParams(window.location.search).has('key')
    };
  }
  return invoke<Bootstrap>('bootstrap');
}

export async function createSession(input: CreateSessionInput): Promise<Session> {
  if (!isNative()) {
    previewSession = {
      ...(previewSession ?? previewState()),
      id: 'simulation-new-note',
      title: input.title || 'Untitled meeting',
      startedAt: '2026-09-13T16:00:00.000Z',
      endedAt: null,
      context: input.context,
      attendees: input.attendees,
      folder: '',
      originalNotes: '',
      enrichedNotes: null,
      status: 'draft'
    };
    return previewSession;
  }
  return invoke<Session>('create_session', { input });
}

export async function saveSession(input: UpdateSessionInput): Promise<Session> {
  if (!isNative()) {
    previewSession = { ...(previewSession ?? previewState()), ...input };
    return previewSession;
  }
  return invoke<Session>('save_session', { input });
}

export async function startRecording(id: string): Promise<RecordingInfo> {
  if (!isNative()) {
    await previewDelay();
    if (new URLSearchParams(window.location.search).get('captureError') === 'permission') {
      throw {
        code: 'audio_permission',
        message: '[SIMULATION] System audio capture was denied with OSStatus -66748.'
      };
    }
    previewSession = { ...(previewSession ?? previewState()), id, status: 'recording', error: null };
    return { sessionId: id, startedAt: new Date().toISOString() };
  }
  return invoke<RecordingInfo>('start_recording', { id });
}

export async function stopRecording(id: string): Promise<Session> {
  if (!isNative()) {
    await previewDelay();
    previewSession = {
      ...(previewSession ?? previewState()),
      id,
      endedAt: new Date().toISOString(),
      status: 'processing',
      error: null
    };
    return previewSession;
  }
  return invoke<Session>('stop_recording', { id });
}

export async function retryProcessing(id: string): Promise<Session> {
  if (!isNative()) {
    await previewDelay();
    previewSession = { ...(previewSession ?? previewState()), id, status: 'processing', error: null };
    return previewSession;
  }
  return invoke<Session>('retry_processing', { id });
}

export async function deleteSession(id: string): Promise<void> {
  if (!isNative()) {
    if (previewSession?.id === id) previewSession = undefined;
    return;
  }
  return invoke<void>('delete_session', { id });
}

export async function deleteTranscript(id: string): Promise<Session> {
  if (!isNative()) {
    previewSession = {
      ...(previewSession ?? previewState()),
      id,
      transcript: null,
      enrichedNotes: null,
      status: 'draft',
      error: null
    };
    return previewSession;
  }
  return invoke<Session>('delete_transcript', { id });
}

export async function saveApiKey(key: string): Promise<void> {
  if (!isNative()) return;
  return invoke<void>('save_api_key', { key });
}

export async function hasApiKey(): Promise<boolean> {
  if (!isNative()) return false;
  return invoke<boolean>('has_api_key');
}

export async function askMeetings(folder: string | null, question: string): Promise<MeetingAnswer> {
  if (!isNative()) {
    await previewDelay();
    const source = previewSession ?? previewState();
    return {
      answer: '[SIMULATION] The team decided to advance to detailed underwriting while the rent roll and roof report remain open.',
      citations: [
        {
          sessionId: source.id,
          title: source.title,
          excerpt: 'Advance to a detailed underwriting review.'
        }
      ]
    };
  }
  return invoke<MeetingAnswer>('ask_meetings', { folder, question });
}

export function disposeAsyncListener(registration: Promise<UnlistenFn>): UnlistenFn {
  let active = true;
  let unlisten: UnlistenFn | undefined;

  registration.then((stop) => {
    if (active) unlisten = stop;
    else stop();
  });

  return () => {
    if (!active) return;
    active = false;
    unlisten?.();
    unlisten = undefined;
  };
}

export function onSessionUpdated(handler: (session: Session) => void): UnlistenFn {
  if (!isNative()) return () => {};
  return disposeAsyncListener(listen<Session>('session-updated', ({ payload }) => handler(payload)));
}

export function onWindowCloseRequested(handler: () => Promise<void>): UnlistenFn {
  if (!isNative()) return () => {};
  return disposeAsyncListener(
    getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      await handler();
    })
  );
}

export function createCloseHandler(
  flush: () => Promise<void>,
  destroy: () => Promise<void>
): () => Promise<void> {
  let closing = false;
  return async () => {
    if (closing) return;
    closing = true;
    try {
      await flush();
      await destroy();
    } catch (error) {
      closing = false;
      throw error;
    }
  };
}

export async function destroyCurrentWindow(): Promise<void> {
  if (isNative()) await getCurrentWindow().destroy();
}

export async function recordingHealth(id: string): Promise<RecordingHealth> {
  if (!isNative()) {
    const source = { admittedFrames: 0, writtenFrames: 0, capturedSeconds: 0, sampleRate: 48_000, lastCallbackAgeSeconds: null, writeError: null, status: 'no_frames' as const };
    return { wallSeconds: 12, system: source, microphone: source, identityChanged: false,
      warnings: ['[SIMULATION] System audio has produced no frames.', '[SIMULATION] Microphone has produced no frames.'] };
  }
  return invoke<RecordingHealth>('recording_health', { id });
}
