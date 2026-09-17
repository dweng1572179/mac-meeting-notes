import { describe, expect, it } from 'vitest';
import { foldersFor, sessionsForFolder, matchesMeeting, meetingStatusLabel } from './library';
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

it('finds names and saved meeting content without requiring accent marks', () => {
  const note = { ...session('one', 'Research'), attendees: ['José'], originalNotes: 'Follow up', transcript: 'Discussed São Paulo financing.' };
  expect(matchesMeeting(note, 'jose')).toBe(true);
  expect(matchesMeeting(note, 'sao paulo')).toBe(true);
  expect(matchesMeeting(note, 'follow up')).toBe(true);
  expect(matchesMeeting(note, 'missing')).toBe(false);
  expect(matchesMeeting(note, '  ')).toBe(true);
});

it('searches the visible AI notes and respects an intentional blank edit', () => {
  const edited = {
    ...session('one', 'Research'),
    enrichedNotes: 'Generated summary phrase',
    editedEnrichedNotes: 'Reviewed decision phrase'
  };
  expect(matchesMeeting(edited, 'reviewed decision')).toBe(true);
  expect(matchesMeeting(edited, 'generated summary')).toBe(false);
  expect(matchesMeeting({ ...edited, editedEnrichedNotes: '' }, 'generated summary')).toBe(false);
});

it('labels silence and interrupted processing as distinct library outcomes', () => {
  expect(meetingStatusLabel({ ...session('one', ''), status: 'failed', error: { code: 'no_speech', message: '' } })).toBe('No speech detected');
  expect(meetingStatusLabel({ ...session('one', ''), status: 'processing' })).toBe('Processing');
  expect(meetingStatusLabel({ ...session('one', ''), status: 'failed' })).toBe('Needs attention');
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
