import { describe, expect, it, vi } from 'vitest';
import { boundedQuestionHistory, createQuestionConversation, createQuestionStore } from './questions';
import type { MeetingAnswer, MeetingQuestionTurn } from './types';

const answer = (text: string): MeetingAnswer => ({ answer: text, citations: [] });
const deferred = () => {
  let resolve!: (value: MeetingAnswer) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<MeetingAnswer>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

describe('question conversation', () => {
  it('keeps the full received answer visible while shortening only follow-up context', async () => {
    const conversation = createQuestionConversation();
    const longAnswer = '🌊'.repeat(13_000);
    conversation.edit('Explain the water cycle.');
    await conversation.ask(async () => answer(longAnswer));
    conversation.edit('Explain the second part.');
    const request = vi.fn(async (_question: string, _history: MeetingQuestionTurn[]) => answer('A shorter explanation.'));
    await conversation.ask(request);
    const history = request.mock.calls[0][1];
    expect(Array.from(history[0].answer).length).toBeLessThanOrEqual(6_000);
    expect(history[0].answer).toContain('[Earlier answer shortened for context]');
    expect(conversation.state.exchanges[0].answer.answer).toBe(longAnswer);
  });

  it('drops oldest turns to respect the UTF-8 context budget without splitting Unicode characters', () => {
    const history = Array.from({ length: 4 }, (_, i) => ({ question: `Question ${i}`, answer: '🌊'.repeat(6_000) }));
    const bounded = boundedQuestionHistory(history);
    const bytes = bounded.reduce((sum, turn) => sum + new TextEncoder().encode(turn.question + turn.answer).length, 0);
    expect(bytes).toBeLessThanOrEqual(60_000);
    expect(bounded.map((turn) => turn.question)).toEqual(['Question 2', 'Question 3']);
    expect(bounded.every((turn) => turn.answer === '🌊'.repeat(6_000))).toBe(true);
  });

  it('starts a new conversation without erasing the draft, and cannot clear an active request', async () => {
    const conversation = createQuestionConversation();
    conversation.edit('An earlier question');
    await conversation.ask(async () => answer('An earlier answer'));
    conversation.edit('Keep this draft');
    const pending = deferred();
    const sent = conversation.ask(() => pending.promise);
    conversation.clear();
    expect(conversation.state.exchanges).toHaveLength(1);
    pending.reject(new Error('Start a new conversation.'));
    await sent;
    conversation.clear();
    expect(conversation.state.question).toBe('Keep this draft');
    expect(conversation.state.exchanges).toEqual([]);
    expect(conversation.state.error).toBe('');
    const next = vi.fn(async () => answer('Fresh answer'));
    await conversation.ask(next);
    expect(next).toHaveBeenCalledWith('Keep this draft', []);
  });

  it('preserves the question and prior answer after an error, and retries with completed history', async () => {
    const conversation = createQuestionConversation();
    conversation.edit('What is evaporation?');
    await conversation.ask(async () => answer('Liquid becomes vapor.'));
    conversation.edit('Which conditions affect it?');
    await conversation.ask(async () => { throw { code: 'openai_network', message: 'Connection lost.' }; });
    expect(conversation.state.question).toBe('Which conditions affect it?');
    expect(conversation.state.exchanges).toHaveLength(1);
    expect(conversation.state.error).toBe('Connection lost.');
    const retry = vi.fn(async () => answer('Temperature is one condition.'));
    await conversation.ask(retry);
    expect(retry).toHaveBeenCalledWith('Which conditions affect it?', [
      { question: 'What is evaporation?', answer: 'Liquid becomes vapor.' }
    ]);
    expect(conversation.state.question).toBe('');
    expect(conversation.state.exchanges).toHaveLength(2);
    expect(conversation.state.error).toBe('');
  });

  it('prevents duplicate requests and keeps a new draft typed while an answer is pending', async () => {
    const conversation = createQuestionConversation();
    const pending = deferred();
    const request = vi.fn(() => pending.promise);
    conversation.edit('First question');
    const sent = conversation.ask(request);
    await conversation.ask(request);
    expect(request).toHaveBeenCalledTimes(1);
    expect(conversation.state.asking).toBe(true);
    conversation.edit('Next question');
    pending.resolve(answer('First answer'));
    await sent;
    expect(conversation.state.question).toBe('Next question');
    expect(conversation.state.exchanges[0].question).toBe('First question');
    expect(conversation.state.asking).toBe(false);
  });

  it.each(['resolve', 'reject'] as const)('detaches the view without delivering a late %s to a different scope', async (outcome) => {
    const changed = vi.fn();
    const oldScope = createQuestionConversation();
    const unsubscribe = oldScope.subscribe(changed);
    const pending = deferred();
    oldScope.edit('Private to the old scope');
    const sent = oldScope.ask(() => pending.promise);
    unsubscribe();
    changed.mockClear();
    const newScope = createQuestionConversation();
    if (outcome === 'resolve') pending.resolve(answer('Old scope answer'));
    else pending.reject(new Error('Old scope failure'));
    await sent;
    expect(changed).not.toHaveBeenCalled();
    expect(newScope.state.exchanges).toEqual([]);
    expect(newScope.state.error).toBe('');
  });

  it('sends only the last four completed turns and no citation metadata', async () => {
    const conversation = createQuestionConversation();
    for (let i = 1; i <= 5; i++) {
      conversation.edit(`Question ${i}`);
      await conversation.ask(async () => ({ answer: `Answer ${i}`, citations: [{ sessionId: 'one', title: 'Synthetic', excerpt: 'Evidence' }] }));
    }
    conversation.edit('Follow-up');
    const request = vi.fn(async () => answer('Sixth answer'));
    await conversation.ask(request);
    expect(request).toHaveBeenCalledWith('Follow-up', [2, 3, 4, 5].map((i) => ({ question: `Question ${i}`, answer: `Answer ${i}` })));
  });
});

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    key: (index: number) => [...values.keys()][index] ?? null,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    removeItem: (key: string) => { values.delete(key); },
    clear: () => values.clear()
  };
}

describe('durable scoped conversations', () => {
  it('keeps a paid request alive when the view leaves and restores its full answer and draft after restart', async () => {
    const storage = memoryStorage();
    const store = createQuestionStore(() => storage);
    const conversation = store.get('meeting:one');
    const view = vi.fn();
    const unsubscribe = conversation.subscribe(view);
    conversation.edit('Explain this.');
    const pending = deferred();
    const sent = conversation.ask(() => pending.promise);
    unsubscribe();
    view.mockClear();
    conversation.edit('My follow-up draft');
    const fullAnswer = { answer: '🌊'.repeat(13_000), citations: [{ sessionId: 'one', title: 'Synthetic', excerpt: 'Original evidence' }] };
    pending.resolve(fullAnswer);
    await sent;
    expect(view).not.toHaveBeenCalled();
    expect(store.get('meeting:one')).toBe(conversation);
    expect(store.get('folder:one').state.exchanges).toEqual([]);
    expect(store.get('all').state.question).toBe('');
    const restored = createQuestionStore(() => storage).get('meeting:one');
    expect(restored.state.exchanges).toEqual([{ question: 'Explain this.', answer: fullAnswer }]);
    expect(restored.state.question).toBe('My follow-up draft');
    expect(restored.state.persistenceError).toBe('');
  });

  it('restores an interrupted question without resending and retries it explicitly without losing a newer draft', async () => {
    const storage = memoryStorage();
    const original = createQuestionStore(() => storage).get('folder:Course');
    original.edit('First question');
    await original.ask(async () => answer('First answer'));
    original.edit('Interrupted question');
    const pending = deferred();
    const sent = original.ask(() => pending.promise);
    original.edit('A newer draft');
    const restored = createQuestionStore(() => storage).get('folder:Course');
    expect(restored.state.asking).toBe(false);
    expect(restored.state.pendingQuestion).toBe('Interrupted question');
    expect(restored.state.error).toMatch(/interrupted/i);
    expect(restored.state.question).toBe('A newer draft');
    const request = vi.fn(async () => answer('Recovered answer'));
    expect(request).not.toHaveBeenCalled();
    await restored.retry(request);
    expect(request).toHaveBeenCalledWith('Interrupted question', [{ question: 'First question', answer: 'First answer' }]);
    expect(restored.state.question).toBe('A newer draft');
    expect(restored.state.pendingQuestion).toBeNull();
    expect(restored.state.exchanges).toHaveLength(2);
    pending.reject(new Error('Original app closed'));
    await sent;
  });

  it('shows persistence failures while retaining full content and can save again after storage recovers', async () => {
    const storage = memoryStorage();
    const setItem = storage.setItem;
    storage.setItem = () => { throw new DOMException('Full', 'QuotaExceededError'); };
    const conversation = createQuestionStore(() => storage).get('all');
    conversation.edit('Keep this question');
    await conversation.ask(async () => answer('Complete answer'.repeat(8_000)));
    expect(conversation.state.exchanges[0].answer.answer).toBe('Complete answer'.repeat(8_000));
    expect(conversation.state.persistenceError).toMatch(/could not be saved/i);
    expect(storage.length).toBe(0);
    storage.setItem = setItem;
    conversation.retrySave();
    expect(conversation.state.persistenceError).toBe('');
    expect(createQuestionStore(() => storage).get('all').state.exchanges[0].answer.answer).toBe('Complete answer'.repeat(8_000));
  });

  it('rejects malformed saved data without destroying the stored copy or crashing the view', () => {
    const storage = memoryStorage();
    createQuestionStore(() => storage).get('all').edit('Saved draft');
    const key = storage.key(0)!;
    storage.setItem(key, JSON.stringify({ version: 1, question: 'Draft', exchanges: [{ answer: { citations: null } }] }));
    const raw = storage.getItem(key);
    const restored = createQuestionStore(() => storage).get('all');
    expect(restored.state.exchanges).toEqual([]);
    expect(restored.state.persistenceError).toMatch(/could not be opened/i);
    restored.edit('Keep new in-memory draft');
    expect(restored.state.question).toBe('Keep new in-memory draft');
    expect(storage.getItem(key)).toBe(raw);
    expect(restored.state.persistenceError).not.toBe('');
  });

  it('deletes meeting and aggregate conversations from memory and disk and ignores their in-flight answers', async () => {
    const storage = memoryStorage();
    const store = createQuestionStore(() => storage);
    for (const scope of ['meeting:one', 'meeting:two', 'all', 'folder:Course']) {
      const conversation = store.get(scope);
      conversation.edit('Synthetic private question');
      await conversation.ask(async () => answer('Synthetic private answer'), [scope === 'meeting:two' ? 'two' : 'one']);
    }
    const aggregate = store.get('all');
    const pending = deferred();
    aggregate.edit('Pending private answer');
    const sent = aggregate.ask(() => pending.promise, ['one']);
    aggregate.edit('Preserve my unrelated draft');
    store.deleteForMeeting('one');
    pending.resolve(answer('Must not reappear'));
    await sent;
    const restored = createQuestionStore(() => storage);
    for (const scope of ['meeting:one', 'all', 'folder:Course']) {
      expect(store.get(scope).state.exchanges).toEqual([]);
      expect(restored.get(scope).state.exchanges).toEqual([]);
      expect(restored.get(scope).state.question).toBe(scope === 'all' ? 'Preserve my unrelated draft' : '');
    }
    expect(restored.get('meeting:two').state.exchanges).toHaveLength(1);
  });

  it('removes saved scopes that are not open and allows a fresh conversation after an unreadable copy is deleted', () => {
    const storage = memoryStorage();
    const oldApp = createQuestionStore(() => storage);
    oldApp.get('meeting:one').edit('Private draft');
    oldApp.get('folder:Private').edit('Private folder draft');
    oldApp.get('meeting:two').edit('Keep this draft');
    const meetingKey = storage.key(0)!;
    storage.setItem(meetingKey, 'invalid saved data');
    const app = createQuestionStore(() => storage);
    const conversation = app.get('meeting:one');
    expect(conversation.state.persistenceError).not.toBe('');
    app.deleteForMeeting('one');
    conversation.edit('Fresh draft after deletion');
    const restarted = createQuestionStore(() => storage);
    expect(restarted.get('meeting:one').state.question).toBe('Fresh draft after deletion');
    expect(restarted.get('folder:Private').state.question).toBe('Private folder draft');
    expect(restarted.get('meeting:two').state.question).toBe('Keep this draft');
  });

  it('preserves unrelated paid conversations and every aggregate draft when deleting source material', async () => {
    const storage = memoryStorage();
    const store = createQuestionStore(() => storage);
    const affected = store.get('folder:Course');
    affected.edit('Course question');
    await affected.ask(async () => answer('Course answer'), ['one', 'two']);
    affected.edit('Keep this course draft');
    const unrelated = store.get('folder:Planning');
    unrelated.edit('Planning question');
    await unrelated.ask(async () => answer('Planning answer'), ['three']);
    unrelated.edit('Keep this planning draft');
    const restarted = createQuestionStore(() => storage);
    restarted.deleteForMeeting('brand-new-draft');
    expect(restarted.get('folder:Course').state.exchanges).toHaveLength(1);
    restarted.deleteForMeeting('one');
    const afterDeletion = createQuestionStore(() => storage);
    expect(afterDeletion.get('folder:Course').state.exchanges).toEqual([]);
    expect(afterDeletion.get('folder:Course').state.question).toBe('Keep this course draft');
    expect(afterDeletion.get('folder:Planning').state.exchanges[0].answer.answer).toBe('Planning answer');
    expect(afterDeletion.get('folder:Planning').state.question).toBe('Keep this planning draft');
  });

  it('reports a failed privacy cleanup and retries even after clearing its in-memory conversation', () => {
    const storage = memoryStorage();
    const store = createQuestionStore(() => storage);
    store.get('meeting:one').edit('Delete this');
    const removeItem = storage.removeItem;
    storage.removeItem = () => { throw new Error('Storage unavailable'); };
    expect(() => store.deleteForMeeting('one')).toThrow(/not been deleted/i);
    expect(store.get('meeting:one').state.question).toBe('');
    storage.removeItem = removeItem;
    store.deleteForMeeting('one');
    expect(createQuestionStore(() => storage).get('meeting:one').state.question).toBe('');
  });
});
