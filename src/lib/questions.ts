import { errorMessage } from './recovery';
import type { MeetingAnswer, MeetingQuestionTurn } from './types';

export type QuestionState = {
  question: string;
  exchanges: { question: string; answer: MeetingAnswer }[];
  asking: boolean;
  error: string;
};
export type AskQuestion = (question: string, history: MeetingQuestionTurn[]) => Promise<MeetingAnswer>;

export function boundedQuestionHistory(history: MeetingQuestionTurn[]): MeetingQuestionTurn[] {
  const marker = '\n[Earlier answer shortened for context]';
  const bounded = history.slice(-4).map(({ question, answer }) => {
    const characters = Array.from(answer);
    return {
      question,
      answer: characters.length <= 6_000 ? answer : characters.slice(0, 6_000 - marker.length).join('') + marker
    };
  });
  const bytes = (turn: MeetingQuestionTurn) => new TextEncoder().encode(turn.question + turn.answer).length;
  let total = bounded.reduce((sum, turn) => sum + bytes(turn), 0);
  while (total > 60_000 && bounded.length) total -= bytes(bounded.shift()!);
  return bounded;
}

export function createQuestionConversation(changed: (state: QuestionState) => void = () => {}) {
  let state: QuestionState = { question: '', exchanges: [], asking: false, error: '' };
  let active = true;
  const update = (next: Partial<QuestionState>) => {
    if (!active) return;
    state = { ...state, ...next };
    changed(state);
  };

  return {
    get state() { return state; },
    edit(question: string) { update({ question, error: '' }); },
    clear() { if (!state.asking) update({ exchanges: [], error: '' }); },
    dispose() { active = false; },
    async ask(request: AskQuestion) {
      const submitted = state.question.trim();
      if (!active || state.asking || !submitted) return;
      const draft = state.question;
      const history = boundedQuestionHistory(state.exchanges.slice(-4).map(({ question, answer }) => ({ question, answer: answer.answer })));
      update({ asking: true, error: '' });
      try {
        const answer = await request(submitted, history);
        if (!active) return;
        update({
          exchanges: [...state.exchanges, { question: submitted, answer }],
          question: state.question === draft ? '' : state.question,
          asking: false
        });
      } catch (error) {
        update({ asking: false, error: errorMessage(error, 'The answer could not be created. Try again.') });
      }
    }
  };
}
