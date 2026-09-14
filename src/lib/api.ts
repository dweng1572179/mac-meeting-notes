import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  Bootstrap,
  CreateSessionInput,
  RecordingInfo,
  Session,
  UpdateSessionInput
} from './types';

const previewSession: Session = {
  id: 'simulation-riverside-logistics',
  title: '[SIMULATION] Riverside Logistics Center',
  startedAt: '2026-09-12T17:30:00.000Z',
  endedAt: '2026-09-12T18:14:00.000Z',
  context: '[SIMULATION] Initial acquisition review for a regional distribution property.',
  attendees: ['Alex Morgan', 'Jordan Lee', 'Sam Rivera'],
  originalNotes:
    '[SIMULATION]\n\nConfirm tenant rollover exposure and request the latest roof inspection. Underwrite the south bay at market rent.',
  transcript: null,
  enrichedNotes:
    '## Decision\n\nAdvance to a detailed underwriting review.\n\n## Follow-ups\n\n- Request the current rent roll and roof report\n- Revisit the south-bay rent assumption',
  status: 'complete',
  error: null,
  audioPath: null
};

const isNative = () => '__TAURI_INTERNALS__' in window;

export async function bootstrap(): Promise<Bootstrap> {
  if (!isNative()) {
    return {
      sessions: new URLSearchParams(window.location.search).has('empty') ? [] : [previewSession],
      hasApiKey: false
    };
  }
  return invoke<Bootstrap>('bootstrap');
}

export async function createSession(input: CreateSessionInput): Promise<Session> {
  if (!isNative()) {
    return {
      ...previewSession,
      id: 'simulation-new-note',
      title: input.title || 'Untitled meeting',
      startedAt: '2026-09-13T16:00:00.000Z',
      endedAt: null,
      context: input.context,
      attendees: input.attendees,
      originalNotes: '',
      enrichedNotes: null,
      status: 'draft'
    };
  }
  return invoke<Session>('create_session', { input });
}

export async function saveSession(input: UpdateSessionInput): Promise<Session> {
  if (!isNative()) return { ...previewSession, ...input };
  return invoke<Session>('save_session', { input });
}

export async function startRecording(id: string): Promise<RecordingInfo> {
  if (!isNative()) return { sessionId: id, startedAt: '2026-09-13T16:00:00.000Z' };
  return invoke<RecordingInfo>('start_recording', { id });
}

export async function stopRecording(id: string): Promise<Session> {
  if (!isNative()) return { ...previewSession, id, status: 'processing' };
  return invoke<Session>('stop_recording', { id });
}

export async function retryProcessing(id: string): Promise<Session> {
  if (!isNative()) return { ...previewSession, id, status: 'processing', error: null };
  return invoke<Session>('retry_processing', { id });
}

export async function deleteSession(id: string): Promise<void> {
  if (!isNative()) return;
  return invoke<void>('delete_session', { id });
}

export async function saveApiKey(key: string): Promise<void> {
  if (!isNative()) return;
  return invoke<void>('save_api_key', { key });
}

export async function hasApiKey(): Promise<boolean> {
  if (!isNative()) return false;
  return invoke<boolean>('has_api_key');
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
