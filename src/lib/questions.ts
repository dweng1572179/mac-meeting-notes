import { errorMessage } from './recovery';
import type { MeetingAnswer, MeetingQuestionTurn } from './types';

export type QuestionState = {
  question: string;
  exchanges: { question: string; answer: MeetingAnswer }[];
  asking: boolean;
  error: string;
  pendingQuestion: string | null;
  sourceIds: string[];
  persistenceError: string;
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

const emptyState = (): QuestionState => ({ question: '', exchanges: [], asking: false, error: '', pendingQuestion: null, sourceIds: [], persistenceError: '' });
const saveError = 'This conversation could not be saved on this device. Keep this window open and copy any needed answers before closing.';
const readError = 'Your saved conversation could not be opened. Its stored copy has been left untouched; new messages are only available in this window.';
type SavedConversation = Omit<QuestionState, 'persistenceError'> & { version: 1 };

function readConversation(raw: string): SavedConversation {
  const saved = JSON.parse(raw);
  const text = (value: unknown) => typeof value === 'string';
  if (saved?.version !== 1 || !text(saved.question) || !text(saved.error) || typeof saved.asking !== 'boolean'
    || !Array.isArray(saved.sourceIds) || !saved.sourceIds.every(text)
    || !(saved.pendingQuestion === null || text(saved.pendingQuestion)) || !Array.isArray(saved.exchanges)
    || !saved.exchanges.every((exchange: QuestionState['exchanges'][number]) => text(exchange?.question)
      && text(exchange?.answer?.answer) && Array.isArray(exchange.answer.citations)
      && exchange.answer.citations.every((citation) => text(citation?.sessionId) && text(citation?.title) && text(citation?.excerpt)))) {
    throw new Error(readError);
  }
  return saved;
}

export function createQuestionConversation(options: {
  initial?: SavedConversation;
  persist?: (state: QuestionState) => void;
  persistenceError?: string;
} = {}) {
  let state: QuestionState = { ...emptyState(), ...options.initial, persistenceError: options.persistenceError ?? '' };
  if (state.asking) state = { ...state, asking: false, error: 'This answer was interrupted when the app closed. Retry when ready; this sends a new request.' };
  let generation = 0;
  const listeners = new Set<(state: QuestionState) => void>();
  const notify = () => listeners.forEach((changed) => changed(state));
  const update = (next: Partial<QuestionState>) => {
    state = { ...state, ...next };
    try {
      options.persist?.(state);
      state = { ...state, persistenceError: '' };
    } catch (error) {
      state = { ...state, persistenceError: error instanceof Error && error.message === readError ? readError : saveError };
    }
    notify();
  };
  async function ask(request: AskQuestion, sourceIds: string[], submitted = state.question.trim()) {
    if (state.asking || !submitted) return;
    const draft = state.question.trim() === submitted ? state.question : null;
    const requestGeneration = generation;
    const history = boundedQuestionHistory(state.exchanges.map(({ question, answer }) => ({ question, answer: answer.answer })));
    update({ asking: true, error: '', pendingQuestion: submitted, sourceIds: [...new Set([...state.sourceIds, ...sourceIds])] });
    try {
      const answer = await request(submitted, history);
      if (generation !== requestGeneration) return;
      update({
        exchanges: [...state.exchanges, { question: submitted, answer }],
        question: state.question === draft ? '' : state.question,
        asking: false,
        pendingQuestion: null
      });
    } catch (error) {
      if (generation === requestGeneration) update({ asking: false, error: errorMessage(error, 'The answer could not be created. Try again.') });
    }
  }
  return {
    get state() { return state; },
    subscribe(changed: (state: QuestionState) => void) {
      listeners.add(changed);
      changed(state);
      return () => { listeners.delete(changed); };
    },
    edit(question: string) { update({ question }); },
    clear() { if (!state.asking) update({ exchanges: [], error: '', pendingQuestion: null, sourceIds: [] }); },
    retrySave() { update({}); },
    discard(keepDraft = false) { generation++; state = { ...emptyState(), question: keepDraft ? state.question : '' }; notify(); },
    ask(request: AskQuestion, sourceIds: string[] = []) { return ask(request, sourceIds); },
    retry(request: AskQuestion, sourceIds: string[] = []) { return ask(request, sourceIds, state.pendingQuestion ?? state.question.trim()); }
  };
}

export function createQuestionStore(storage: () => Storage) {
  const prefix = 'meeting-notes.questions.v1:';
  const conversations = new Map<string, ReturnType<typeof createQuestionConversation>>();
  return {
    get(scope: string) {
      const existing = conversations.get(scope);
      if (existing) return existing;
      const key = prefix + scope;
      let initial: SavedConversation | undefined;
      let failedRead = false;
      try {
        const raw = storage().getItem(key);
        if (raw !== null) initial = readConversation(raw);
      } catch { failedRead = true; }
      const conversation = createQuestionConversation({
        initial,
        persistenceError: failedRead ? readError : '',
        persist(state) {
          // Leave unreadable data intact, rather than overwrite saved work with an empty thread.
          if (failedRead && storage().getItem(key) !== null) throw new Error(readError);
          failedRead = false;
          const { persistenceError: _error, ...saved } = state;
          storage().setItem(key, JSON.stringify({ ...saved, version: 1 }));
        }
      });
      conversations.set(scope, conversation);
      return conversation;
    },
    deleteForMeeting(id: string) {
      try {
        const saved = storage();
        const scopes = new Set(conversations.keys());
        for (let index = 0; index < saved.length; index++) {
          const key = saved.key(index);
          if (key?.startsWith(prefix)) scopes.add(key.slice(prefix.length));
        }
        const affected: { scope: string; conversation: ReturnType<typeof createQuestionConversation>; meeting: boolean }[] = [];
        for (const scope of scopes) {
          const meeting = scope === `meeting:${id}`;
          if (!meeting && scope !== 'all' && !scope.startsWith('folder:')) continue;
          const raw = saved.getItem(prefix + scope);
          const persisted = raw && !meeting ? readConversation(raw) : null;
          const current = conversations.get(scope);
          // Membership includes candidates that may finish processing before the next UI update, not just cited excerpts.
          if (meeting || current?.state.sourceIds.includes(id) || persisted?.sourceIds.includes(id)) {
            affected.push({ scope, conversation: current ?? this.get(scope), meeting });
          }
        }
        // Invalidate pending responses before removing any saved content.
        for (const { conversation, meeting } of affected) conversation.discard(!meeting);
        for (const { scope, conversation, meeting } of affected) {
          if (meeting || !conversation.state.question) saved.removeItem(prefix + scope);
          else {
            const { persistenceError: _error, ...state } = conversation.state;
            saved.setItem(prefix + scope, JSON.stringify({ ...state, version: 1 }));
          }
        }
      } catch {
        throw new Error('Saved AI answers could not be removed from this device. The meeting and transcript have not been deleted. Try deleting again.');
      }
    }
  };
}

export const questionConversations = createQuestionStore(() => window.localStorage);
