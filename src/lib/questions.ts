import { errorMessage } from './recovery';
import type { MeetingAnswer, MeetingQuestionTurn } from './types';

export type QuestionState = {
  question: string;
  exchanges: { question: string; answer: MeetingAnswer }[];
  asking: boolean;
  error: string;
};
export type AskQuestion = (question: string, history: MeetingQuestionTurn[]) => Promise<MeetingAnswer>;

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
    dispose() { active = false; },
    async ask(request: AskQuestion) {
      const submitted = state.question.trim();
      if (!active || state.asking || !submitted) return;
      const draft = state.question;
      const history = state.exchanges.slice(-4).map(({ question, answer }) => ({ question, answer: answer.answer }));
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
