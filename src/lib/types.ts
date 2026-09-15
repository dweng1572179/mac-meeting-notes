export type SessionStatus = 'draft' | 'recording' | 'processing' | 'complete' | 'failed';

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
  status: SessionStatus;
  error: { code: string; message: string } | null;
  audioPath: string | null;
  microphoneAudioPath: string | null;
};

export type Bootstrap = { sessions: Session[]; hasApiKey: boolean };
export type CreateSessionInput = Pick<Session, 'title' | 'context' | 'attendees'>;
export type UpdateSessionInput = Pick<
  Session,
  'id' | 'title' | 'context' | 'attendees' | 'folder' | 'originalNotes'
>;

export type RecordingInfo = { sessionId: string; startedAt: string };

export type MeetingCitation = { sessionId: string; title: string; excerpt: string };
export type MeetingAnswer = { answer: string; citations: MeetingCitation[] };
