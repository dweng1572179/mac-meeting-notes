import type { Session } from './types';

export function foldersFor(sessions: Session[]): string[] {
  return [...new Set(sessions.map(({ folder }) => folder.trim()).filter(Boolean))].sort((left, right) =>
    left.localeCompare(right)
  );
}

export function sessionsForFolder(sessions: Session[], folder: string | null): Session[] {
  return folder === null ? sessions : sessions.filter((session) => session.folder === folder);
}

export function matchesMeeting(session: Session, query: string): boolean {
  const normalize = (text: string) => text.normalize('NFD').replace(/\p{M}/gu, '').toLocaleLowerCase();
  const needle = normalize(query.trim());
  if (!needle) return true;
  // ponytail: linear local search fits a personal library; index only when library size makes typing slow.
  return [session.title, session.context, session.folder, ...session.attendees, session.originalNotes,
    session.enrichedNotes ?? '', session.transcript ?? ''].some((value) => normalize(value).includes(needle));
}

export function meetingStatusLabel(session: Session): string {
  if (session.error?.code === 'no_speech') return 'No speech detected';
  return { draft: 'Draft', recording: 'Recording', processing: 'Processing', complete: 'Notes ready', failed: 'Needs attention' }[session.status];
}
