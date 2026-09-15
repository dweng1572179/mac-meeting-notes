import type { Session } from './types';

export function foldersFor(sessions: Session[]): string[] {
  return [...new Set(sessions.map(({ folder }) => folder.trim()).filter(Boolean))].sort((left, right) =>
    left.localeCompare(right)
  );
}

export function sessionsForFolder(sessions: Session[], folder: string | null): Session[] {
  return folder === null ? sessions : sessions.filter((session) => session.folder === folder);
}
