import { meetingNotes } from './meeting-workspace.ts';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  Bootstrap,
  CreateSessionInput,
  MeetingAnswer,
  MeetingQuestionTurn,
  RecordingInfo,
  RecordingHealth,
  Session,
  TranscriptionSettings,
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
  originalNotes: '',
  transcript: '[SIMULATION]\n\n[00:00:04 – 00:00:22] Microphone:\nThe current rent roll is still outstanding, so we should not finalize underwriting today.\n\n[00:00:24 – 00:00:47] System audio:\nAgreed. We can advance to detailed review while the roof report and tenant rollover schedule remain open.\n\n[00:00:49 – 00:01:08] Microphone:\nJordan will request both documents by Friday, and Alex will revisit the south-bay rent assumption.',
  enrichedNotes:
    '## Decision\n\nAdvance to detailed underwriting, but do not finalize until the rent roll and roof report are reviewed.\n\n## Follow-ups\n\n- Priya: request the current rent roll and roof report by Friday\n- Alex: revisit the south-bay rent assumption\n\n## Open questions\n\n- Confirm tenant rollover exposure after the updated schedule arrives',
  editedEnrichedNotes: null,
  aiSuggestions: {
    title: { value: '[SIMULATION] Riverside underwriting review', evidence: [{ sourceId: 'microphone:segment-0:offset-0:turn-a', excerpt: 'The current rent roll is still outstanding' }] },
    context: { value: '[SIMULATION] Review open diligence items before final underwriting.', evidence: [{ sourceId: 'system:segment-0:offset-0:turn-b', excerpt: 'the roof report and tenant rollover schedule remain open' }] },
    category: { value: 'Underwriting', evidence: [{ sourceId: 'system:segment-0:offset-0:turn-b', excerpt: 'advance to detailed review' }] },
    participants: [
      { name: 'Priya Shah', speakerKey: 'microphone:segment-1:offset-48000:B', evidence: [{ sourceId: 'microphone:segment-1:offset-48000:turn-c', excerpt: 'Priya will request both documents by Friday' }] }
    ],
    topics: [
      { title: 'Open diligence', startsAtTurnId: 'microphone:segment-0:offset-0:turn-a', evidence: [{ sourceId: 'microphone:segment-0:offset-0:turn-a', excerpt: 'rent roll is still outstanding' }] },
      { title: 'Owners and next steps', startsAtTurnId: 'microphone:segment-1:offset-48000:turn-c', evidence: [{ sourceId: 'microphone:segment-1:offset-48000:turn-c', excerpt: 'Jordan will request both documents by Friday' }] }
    ]
  },
  dismissedSuggestions: [],
  transcription: [
    { source: 'microphone', chunks: [
      { startSeconds: 0, durationSeconds: 24, segmentIndex: 0, transcript: 'The current rent roll is still outstanding, so we should not finalize underwriting today.', segments: [
        { id: 'turn-a', speaker: 'A', startSeconds: 4, endSeconds: 22, text: 'The current rent roll is still outstanding, so we should not finalize underwriting today.' }
      ] },
      { startSeconds: 48, durationSeconds: 24, segmentIndex: 1, transcript: 'Priya will request both documents by Friday, and Alex will revisit the south-bay rent assumption.', segments: [
        { id: 'turn-c', speaker: 'B', startSeconds: 1, endSeconds: 20, text: 'Priya will request both documents by Friday, and Alex will revisit the south-bay rent assumption.' }
      ] }
    ] },
    { source: 'system', chunks: [
      { startSeconds: 0, durationSeconds: 48, segmentIndex: 0, transcript: 'Agreed. We can advance to detailed review while the roof report and tenant rollover schedule remain open.', segments: [
        { id: 'turn-b', speaker: 'A', startSeconds: 24, endSeconds: 47, text: 'Agreed. We can advance to detailed review while the roof report and tenant rollover schedule remain open.' }
      ] }
    ] }
  ],
  status: 'complete',
  error: null,
  audioPath: null,
  microphoneAudioPath: null
};

const isNative = () => '__TAURI_INTERNALS__' in window;
let previewSessions: Map<string, Session> | undefined;
const failedPreviewQuestions = new Set<string>();
export const defaultTranscriptionSettings: TranscriptionSettings = { language: '', vocabulary: '', model: 'gpt-4o-transcribe-diarize' };
let previewSettings = { ...defaultTranscriptionSettings };
let previewHasApiKey: boolean | undefined;

function previewState(): Session {
  const failure = new URLSearchParams(window.location.search).get('failure');
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
      && failure !== 'no_speech' ? basePreviewSession.transcript
      : null,
    transcription: ['complete', 'failed'].includes(selectedStatus) && failure !== 'no_speech'
      ? basePreviewSession.transcription
      : [],
    aiSuggestions: selectedStatus === 'complete' ? basePreviewSession.aiSuggestions : null,
    dismissedSuggestions: [],
    editedEnrichedNotes: null,
    enrichedNotes: selectedStatus === 'complete' ? basePreviewSession.enrichedNotes : null,
    status: selectedStatus,
    error:
      selectedStatus === 'failed'
        ? failure === 'no_speech' ? { code: 'no_speech', message: 'No speech was detected. Your typed notes and source audio were kept.' }
          : failure === 'invalid_api_key' ? { code: 'invalid_api_key', message: 'OpenAI rejected this API key. Request ID: req_simulation' }
          : { code: 'openai_request', message: '[SIMULATION] Processing could not reach OpenAI.' }
        : null,
    audioPath: ['recording', 'processing', 'failed'].includes(selectedStatus)
      ? '/tmp/simulation-riverside-logistics.m4a'
      : null,
    microphoneAudioPath: ['recording', 'processing', 'failed'].includes(selectedStatus)
      ? '/tmp/simulation-riverside-logistics-mic.m4a'
      : null
  };
}

const previewDelay = (ms = 350) => new Promise((resolve) => setTimeout(resolve, ms));

function previewLibrary() {
  if (!previewSessions) {
    const query = new URLSearchParams(window.location.search);
    const initial = previewState();
    previewSessions = new Map(query.has('empty') ? [] : [[initial.id, initial]]);
    if (!query.has('empty') && query.get('meetings') === '2') {
      const other: Session = { ...previewState(), id: 'simulation-harbor-planning', title: '[SIMULATION] Harbor planning', folder: 'Planning', context: '', attendees: [], status: 'complete', notes: '[SIMULATION] Maya will prepare the launch checklist by Tuesday.', transcript: null, enrichedNotes: null, transcription: [], aiSuggestions: null };
      previewSessions.set(other.id, other);
    }
  }
  return previewSessions;
}

function previewSession(id: string): Session {
  const session = previewLibrary().get(id);
  if (!session) throw new Error('[SIMULATION] This meeting no longer exists.');
  return session;
}

function updatePreviewSession(session: Session): Session {
  previewLibrary().set(session.id, session);
  return session;
}

async function previewAnswer(scope: string, sources: Session[], question: string, history: MeetingQuestionTurn[] = []): Promise<MeetingAnswer> {
  const query = new URLSearchParams(window.location.search);
  const requestedDelay = Number(query.get('askDelay') ?? 350);
  await previewDelay(Number.isFinite(requestedDelay) ? Math.max(0, Math.min(requestedDelay, 60_000)) : 350);
  const requestKey = JSON.stringify([scope, question]);
  if (query.get('askError') === 'always' || (query.get('askError') === 'once' && !failedPreviewQuestions.has(requestKey))) {
    failedPreviewQuestions.add(requestKey);
    throw new Error('[SIMULATION] Connection interrupted. Your question is ready to retry.');
  }
  const available = sources.filter((source) => previewLibrary().has(source.id) && source.status === 'complete' && (meetingNotes(source).trim() || source.transcript?.trim() || source.context.trim()));
  if (!available.length) throw new Error('[SIMULATION] No completed meetings with source material are available here yet.');
  const source = available[0];
  const evidence = meetingNotes(source).trim() || source.transcript?.trim() || source.context.trim();
  const excerpt = evidence.split(/\n\s*\n/u).find((paragraph) => !paragraph.startsWith('#')) ?? evidence;
  const response = `[SIMULATION] ${history.length ? 'For this follow-up, the saved evidence still says:' : 'The saved meeting says:'}\n\n${excerpt}\n\nThis preview uses synthetic local content and does not contact OpenAI.`;
  return {
    answer: query.get('answer') === 'long' ? Array.from({ length: 40 }, (_, i) => `## Detail ${i + 1}\n\n${response}`).join('\n\n') : response,
    citations: [{ sessionId: source.id, title: source.title, excerpt }]
  };
}

export async function bootstrap(): Promise<Bootstrap> {
  if (!isNative()) {
    return {
      sessions: structuredClone([...previewLibrary().values()]),
      hasApiKey: previewHasApiKey ?? new URLSearchParams(window.location.search).has('key'),
      settings: previewSettings,
      keyAccessError: new URLSearchParams(window.location.search).has('keyError') ? '[SIMULATION] Your system credential store could not be opened. Local notes are still available.' : null,
      dataDirectory: '[SIMULATION] Local application data folder'
    };
  }
  return invoke<Bootstrap>('bootstrap');
}

export async function createSession(input: CreateSessionInput): Promise<Session> {
  if (!isNative()) {
    return updatePreviewSession({
      ...previewState(),
      id: `simulation-${crypto.randomUUID()}`,
      title: input.title || 'Untitled meeting',
      startedAt: new Date().toISOString(),
      endedAt: null,
      context: input.context,
      attendees: input.attendees,
      folder: '',
      originalNotes: '',
      notes: null,
      previousNotes: null,
      enrichedNotes: null,
      transcript: null,
      audioPath: null,
      microphoneAudioPath: null,
      error: null,
      warnings: [],
      transcription: [],
      aiSuggestions: null,
      dismissedSuggestions: [],
      editedEnrichedNotes: null,
      captureSegments: [],
      segmentedCapture: false,
      liveTranscriptionError: null,
      captureHealth: null,
      status: 'draft'
    });
  }
  return invoke<Session>('create_session', { input });
}

export async function saveSession(input: UpdateSessionInput): Promise<Session> {
  if (!isNative()) {
    const { notes, ...metadata } = input;
    return updatePreviewSession({ ...previewSession(input.id), ...metadata, ...(notes !== undefined ? { notes } : {}) });
  }
  return invoke<Session>('save_session', { input });
}

export async function applySuggestion(id: string, key: string, action: 'apply' | 'dismiss'): Promise<Session> {
  if (!isNative()) {
    const source = previewSession(id);
    const next = { ...source, id, dismissedSuggestions: [...new Set([...(source.dismissedSuggestions ?? []), key])] };
    if (action === 'apply' && source.aiSuggestions) {
      const suggestions = source.aiSuggestions;
      if (key === 'title' && suggestions.title) next.title = suggestions.title.value;
      if (key === 'context' && suggestions.context) next.context = suggestions.context.value;
      if (key === 'category' && suggestions.category) next.folder = suggestions.category.value;
      if (key === 'participants') next.attendees = [...new Set([...source.attendees, ...suggestions.participants.map(({ name }) => name)])];
    }
    return updatePreviewSession(next);
  }
  return invoke<Session>('apply_suggestion', { id, key, action });
}

export async function refreshInsights(id: string): Promise<Session> {
  if (!isNative()) {
    await previewDelay();
    const source = previewSession(id);
    if (!source.transcript || source.status !== 'complete') throw new Error('[SIMULATION] This meeting is not ready to update notes.');
    return updatePreviewSession({ ...source, notes: `[SIMULATION] Updated from the saved transcript.\n\n${source.transcript}`, previousNotes: meetingNotes(source) });
  }
  return invoke<Session>('refresh_insights', { id });
}

export async function restoreNotes(id: string, expectedNotes: string): Promise<Session> {
  if (!isNative()) {
    const source = previewSession(id);
    if (source.status === 'recording' || source.status === 'processing' || source.previousNotes == null || meetingNotes(source) !== expectedNotes) {
      throw new Error('[SIMULATION] The previous version cannot be restored while notes have changed or processing is active.');
    }
    return updatePreviewSession({ ...source, notes: source.previousNotes, previousNotes: meetingNotes(source) });
  }
  return invoke<Session>('restore_notes', { id, expectedNotes });
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
    updatePreviewSession({ ...previewSession(id), status: 'recording', error: null });
    return { sessionId: id, startedAt: new Date().toISOString() };
  }
  return invoke<RecordingInfo>('start_recording', { id });
}

export async function stopRecording(id: string): Promise<Session> {
  if (!isNative()) {
    await previewDelay();
    return updatePreviewSession({
      ...previewSession(id),
      endedAt: new Date().toISOString(),
      status: 'processing',
      error: null
    });
  }
  return invoke<Session>('stop_recording', { id });
}

export async function retryProcessing(id: string): Promise<Session> {
  if (!isNative()) {
    await previewDelay();
    return updatePreviewSession({ ...previewSession(id), status: 'processing', error: null });
  }
  return invoke<Session>('retry_processing', { id });
}

export async function deleteSession(id: string): Promise<void> {
  if (!isNative()) {
    previewLibrary().delete(id);
    return;
  }
  return invoke<void>('delete_session', { id });
}

export async function deleteTranscript(id: string): Promise<Session> {
  if (!isNative()) {
    const source = previewSession(id);
    return updatePreviewSession({
      ...source,
      id,
      transcript: null,
      enrichedNotes: null,
      notes: meetingNotes(source),
      editedEnrichedNotes: null,
      aiSuggestions: null,
      dismissedSuggestions: [],
      transcription: [],
      captureSegments: [],
      segmentedCapture: false,
      liveTranscriptionError: null,
      status: 'draft',
      error: null
    });
  }
  return invoke<Session>('delete_transcript', { id });
}

export async function saveApiKey(key: string): Promise<void> {
  if (!isNative()) { previewHasApiKey = true; return; }
  return invoke<void>('save_api_key', { key });
}

export async function removeApiKey(): Promise<void> {
  if (!isNative()) { previewHasApiKey = false; return; }
  return invoke<void>('remove_api_key');
}

export type SettingsDestination = 'api-keys' | 'billing' | 'microphone' | 'system-audio' | 'data-folder';
export async function openSettingsDestination(destination: SettingsDestination): Promise<void> {
  if (!isNative()) throw new Error('[SIMULATION] This action is available in the installed desktop app.');
  return invoke<void>('open_settings_destination', { destination });
}

export async function hasApiKey(): Promise<boolean> {
  if (!isNative()) return previewHasApiKey ?? new URLSearchParams(window.location.search).has('key');
  return invoke<boolean>('has_api_key');
}

export async function saveTranscriptionSettings(settings: TranscriptionSettings): Promise<TranscriptionSettings> {
  if (!isNative()) { previewSettings = { ...settings }; return previewSettings; }
  return invoke<TranscriptionSettings>('save_transcription_settings', { settings });
}

export async function askMeeting(id: string, question: string, history?: MeetingQuestionTurn[]): Promise<MeetingAnswer> {
  if (!isNative()) return previewAnswer(`meeting:${id}`, [previewSession(id)], question, history);
  return invoke<MeetingAnswer>('ask_meetings', { folder: null, sessionId: id, question, ...(history ? { history: history.slice(-4) } : {}) });
}

export async function exportMarkdown(title: string, markdown: string): Promise<string> {
  if (!isNative()) {
    const url = URL.createObjectURL(new Blob([markdown], { type: 'text/markdown;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${title.replace(/[^\p{L}\p{N} _-]/gu, '').slice(0, 80) || 'Meeting'}.md`;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    return 'your browser’s downloads';
  }
  return invoke<string>('export_markdown', { title, markdown });
}

export async function askMeetings(folder: string | null, question: string, history?: MeetingQuestionTurn[]): Promise<MeetingAnswer> {
  if (!isNative()) {
    const sources = [...previewLibrary().values()].filter((source) => folder === null || source.folder === folder);
    return previewAnswer(folder === null ? 'all' : `folder:${folder}`, sources, question, history);
  }
  return invoke<MeetingAnswer>('ask_meetings', { folder, question, ...(history ? { history: history.slice(-4) } : {}) });
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
