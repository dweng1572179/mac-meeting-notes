import { afterEach, describe, expect, it, vi } from 'vitest';
import { createAutosave } from './autosave';

afterEach(() => vi.useRealTimers());

describe('createAutosave', () => {
  it('saves only the latest draft after 450 milliseconds of inactivity', async () => {
    vi.useFakeTimers();
    const saved: string[] = [];
    const autosave = createAutosave<string>(450, async (draft) => void saved.push(draft));

    autosave.schedule('early');
    await vi.advanceTimersByTimeAsync(300);
    autosave.schedule('latest');
    await vi.advanceTimersByTimeAsync(449);
    expect(saved).toEqual([]);

    await vi.advanceTimersByTimeAsync(1);
    expect(saved).toEqual(['latest']);
  });

  it('flushes a pending draft without waiting for its timer', async () => {
    vi.useFakeTimers();
    const saved: string[] = [];
    const autosave = createAutosave<string>(450, async (draft) => void saved.push(draft));

    autosave.schedule('before action');
    await autosave.flush();

    expect(saved).toEqual(['before action']);
  });
});
