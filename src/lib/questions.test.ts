import { describe, expect, it, vi } from 'vitest';
import { boundedQuestionHistory, createQuestionConversation } from './questions';
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

  it.each(['resolve', 'reject'] as const)('ignores a late %s after leaving the meeting or folder', async (outcome) => {
    const changed = vi.fn();
    const oldScope = createQuestionConversation(changed);
    const pending = deferred();
    oldScope.edit('Private to the old scope');
    const sent = oldScope.ask(() => pending.promise);
    oldScope.dispose();
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
