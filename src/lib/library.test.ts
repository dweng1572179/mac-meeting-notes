import { describe, expect, it } from 'vitest';
import { foldersFor, sessionsForFolder } from './library';
import type { Session } from './types';

const session = (id: string, folder: string): Session => ({
  id,
  title: id,
  startedAt: '2026-09-15T10:00:00Z',
  endedAt: null,
  context: '',
  attendees: [],
  folder,
  originalNotes: '',
  transcript: null,
  enrichedNotes: null,
  status: 'draft',
  error: null,
  audioPath: null,
  microphoneAudioPath: null
});

describe('meeting folders', () => {
  const sessions = [
    session('uncategorized', ''),
    session('second-acquisition', 'Acquisitions'),
    session('leasing', 'Leasing'),
    session('first-acquisition', 'Acquisitions')
  ];

  it('derives unique non-empty folder names in display order', () => {
    expect(foldersFor(sessions)).toEqual(['Acquisitions', 'Leasing']);
  });

  it('filters a folder exactly while null keeps all meetings', () => {
    expect(sessionsForFolder(sessions, 'Acquisitions').map(({ id }) => id)).toEqual([
      'second-acquisition',
      'first-acquisition'
    ]);
    expect(sessionsForFolder(sessions, null)).toHaveLength(4);
  });
});
