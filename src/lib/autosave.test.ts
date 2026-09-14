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

  it('waits for a new inactivity window when input arrives during a timer save', async () => {
    vi.useFakeTimers();
    const saved: string[] = [];
    let finishFirst!: () => void;
    const firstSave = new Promise<void>((resolve) => (finishFirst = resolve));
    const autosave = createAutosave<string>(450, async (draft) => {
      saved.push(draft);
      if (draft === 'first') await firstSave;
    });

    autosave.schedule('first');
    await vi.advanceTimersByTimeAsync(450);
    autosave.schedule('second');
    finishFirst();
    await vi.advanceTimersByTimeAsync(449);

    expect(saved).toEqual(['first']);
    await vi.advanceTimersByTimeAsync(1);
    expect(saved).toEqual(['first', 'second']);
  });

  it('flushes input queued during an in-flight timer save', async () => {
    vi.useFakeTimers();
    const saved: string[] = [];
    let finishFirst!: () => void;
    const firstSave = new Promise<void>((resolve) => (finishFirst = resolve));
    const autosave = createAutosave<string>(450, async (draft) => {
      saved.push(draft);
      if (draft === 'first') await firstSave;
    });

    autosave.schedule('first');
    await vi.advanceTimersByTimeAsync(450);
    autosave.schedule('second');
    const flushed = autosave.flush();
    finishFirst();
    await flushed;

    expect(saved).toEqual(['first', 'second']);
  });

  it('waits for a due timer save that overlaps an explicit flush', async () => {
    vi.useFakeTimers();
    const saved: string[] = [];
    let finishFirst!: () => void;
    let finishSecond!: () => void;
    const firstSave = new Promise<void>((resolve) => (finishFirst = resolve));
    const secondSave = new Promise<void>((resolve) => (finishSecond = resolve));
    const autosave = createAutosave<string>(450, async (draft) => {
      saved.push(draft);
      await (draft === 'first' ? firstSave : secondSave);
    });

    autosave.schedule('first');
    await vi.advanceTimersByTimeAsync(450);
    autosave.schedule('second');
    await vi.advanceTimersByTimeAsync(450);
    const flushed = autosave.flush();
    finishFirst();
    for (let step = 0; step < 10; step++) await Promise.resolve();
    let flushFinished = false;
    void flushed.then(() => (flushFinished = true));
    await Promise.resolve();

    expect(saved).toEqual(['first', 'second']);
    expect(flushFinished).toBe(false);
    finishSecond();
    await flushed;
  });

  it('does not let a due timer consume newer input queued behind a blocked save', async () => {
    vi.useFakeTimers();
    const saved: string[] = [];
    let finishFirst!: () => void;
    const firstSave = new Promise<void>((resolve) => (finishFirst = resolve));
    const autosave = createAutosave<string>(450, async (draft) => {
      saved.push(draft);
      if (draft === 'first') await firstSave;
    });

    autosave.schedule('first');
    await vi.advanceTimersByTimeAsync(450);
    autosave.schedule('due');
    await vi.advanceTimersByTimeAsync(450);
    autosave.schedule('newer');
    finishFirst();
    for (let step = 0; step < 10; step++) await Promise.resolve();

    expect(saved).toEqual(['first', 'due']);
    await vi.advanceTimersByTimeAsync(449);
    expect(saved).toEqual(['first', 'due']);
    await vi.advanceTimersByTimeAsync(1);
    expect(saved).toEqual(['first', 'due', 'newer']);
  });
});
