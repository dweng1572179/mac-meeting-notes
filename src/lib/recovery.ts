import type { Session } from './types';

export function errorMessage(error: unknown, fallback = 'The action could not be completed.'): string {
  const detail = error && typeof error === 'object' && 'message' in error && typeof error.message === 'string'
    ? error.message : typeof error === 'string' && error.trim() ? error : fallback;
  const code = error && typeof error === 'object' && 'code' in error ? error.code : '';
  if (code === 'microphone_permission') return `${detail} Open System Settings → Privacy & Security → Microphone, enable Meeting Notes, then try again.`;
  if (code === 'audio_permission') return `${detail} Open System Settings → Privacy & Security → Screen & System Audio Recording, enable Meeting Notes, then try again.`;
  return detail;
}

export function recoveryActions(session: Session) {
  const code = session.error?.code;
  const audio = Boolean(session.audioPath || session.microphoneAudioPath);
  const text = Boolean(session.transcript?.trim());
  const chunks = session.transcription?.flatMap((track) => track.chunks);
  const pending = chunks?.some((chunk) => chunk.transcript === null);
  const noSpeech = code === 'no_speech';
  const retryable = noSpeech ? audio && (!chunks || chunks.length > 0) : audio || text;
  return {
    settings: ['invalid_api_key', 'missing_api_key', 'model_access', 'permission_denied'].includes(code ?? ''),
    newMeeting: noSpeech || !retryable,
    retryLabel: !retryable ? null : noSpeech ? 'Retry transcription' : text && !pending ? 'Retry notes' : 'Resume processing',
    guidance: noSpeech
      ? 'Silence is a valid recording outcome. If speech was expected, check the microphone and audio source. A new meeting starts a fresh recording; retry only reprocesses saved audio.'
      : text && !pending
        ? 'Your transcript and original notes are saved. Retry continues with enhanced notes.'
        : retryable ? 'Your original notes and saved progress are kept. Retry resumes unfinished work.'
        : 'Your original notes are kept. Create a new meeting to record again.'
  };
}

export function processingLabel(session: Session): string {
  const chunks = session.transcription?.flatMap((track) => track.chunks) ?? [];
  const saved = chunks.filter((chunk) => chunk.transcript !== null).length;
  if (chunks.length > saved) return `Transcribing · ${saved} of ${chunks.length} parts saved`;
  if (session.transcript?.trim()) return 'Writing enhanced notes…';
  return chunks.length ? 'Finishing transcription…' : 'Preparing audio…';
}
