export type SessionStatus = 'draft' | 'recording' | 'processing' | 'complete' | 'failed';

export type Evidence = { sourceId: string; excerpt: string };
export type Suggestion = { value: string; evidence: Evidence[] };
export type ParticipantSuggestion = { name: string; speakerKey: string | null; evidence: Evidence[] };
export type TopicSuggestion = { title: string; startsAtTurnId: string; evidence: Evidence[] };
export type AiSuggestions = {
  title: Suggestion | null;
  context: Suggestion | null;
  category: Suggestion | null;
  participants: ParticipantSuggestion[];
  topics: TopicSuggestion[];
};
export type TranscriptSegment = { id: string; speaker: string; startSeconds: number; endSeconds: number; text: string };
export type TranscriptChunk = {
  startSeconds: number;
  durationSeconds: number;
  transcript: string | null;
  segmentIndex?: number | null;
  segments?: TranscriptSegment[];
};
export type CaptureSegment = { source: 'system' | 'microphone'; index: number; startSeconds: number; durationSeconds: number };

export type Session = {
  id: string;
  title: string;
  startedAt: string;
  endedAt: string | null;
  context: string;
  attendees: string[];
  folder: string;
  originalNotes: string;
  transcript: string | null;
  enrichedNotes: string | null;
  editedEnrichedNotes?: string | null;
  aiSuggestions?: AiSuggestions | null;
  dismissedSuggestions?: string[];
  segmentedCapture?: boolean;
  captureSegments?: CaptureSegment[];
  liveTranscriptionError?: { code: string; message: string } | null;
  status: SessionStatus;
  error: { code: string; message: string } | null;
  audioPath: string | null;
  microphoneAudioPath: string | null;
  warnings?: string[];
  captureHealth?: RecordingHealth | null;
  transcriptionSettings?: TranscriptionSettings;
  transcription?: {
    source: 'system' | 'microphone';
    chunks: TranscriptChunk[];
  }[];
};

export type TranscriptionSettings = { language: string; vocabulary: string; model: 'gpt-4o-mini-transcribe' | 'gpt-4o-transcribe' | 'gpt-4o-transcribe-diarize' };
export type Bootstrap = { sessions: Session[]; hasApiKey: boolean; settings: TranscriptionSettings };
export type CreateSessionInput = Pick<Session, 'title' | 'context' | 'attendees'>;
export type UpdateSessionInput = Pick<
  Session,
  'id' | 'title' | 'context' | 'attendees' | 'folder' | 'originalNotes'
>;

export type RecordingInfo = { sessionId: string; startedAt: string };

export type MeetingCitation = { sessionId: string; title: string; excerpt: string };
export type MeetingAnswer = { answer: string; citations: MeetingCitation[] };

export type SourceHealth = {
  admittedFrames: number;
  writtenFrames: number;
  capturedSeconds: number;
  sampleRate: number;
  lastCallbackAgeSeconds: number | null;
  writeError: number | null;
  status: 'starting' | 'healthy' | 'no_frames' | 'stalled' | 'write_error' | 'short_capture';
};
export type RecordingHealth = {
  wallSeconds: number;
  system: SourceHealth;
  microphone: SourceHealth;
  identityChanged: boolean;
  warnings: string[];
};
