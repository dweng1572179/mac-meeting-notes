<script lang="ts">
  import QuestionThread from './QuestionThread.svelte';
  import type { AskQuestion } from './questions';

  let { sessionId, hasApiKey, onAsk, onOpenSettings, topic }: {
    sessionId: string;
    hasApiKey: boolean;
    onAsk: AskQuestion;
    onOpenSettings: () => void;
    topic?: string;
  } = $props();
  let expanded = $state(false);
  const starters = $derived([
    'Summarize the main ideas.',
    topic?.trim() && topic.length <= 100 ? `Explain “${topic.trim()}”.` : 'What details should I remember?'
  ]);
</script>

<aside class="question-panel meeting-question" class:expanded aria-label="Ask this meeting">
  <button class="question-panel-toggle" type="button" aria-expanded={expanded} aria-controls="meeting-question-panel" onclick={() => (expanded = !expanded)}>
    <span>Ask this meeting</span>
    <svg aria-hidden="true" viewBox="0 0 20 20"><path d="m5 12 5-5 5 5" /></svg>
  </button>
  <div class="question-panel-content" id="meeting-question-panel">
    <header class="question-panel-heading">
      <h2>Ask this meeting</h2>
      <p>From your notes and transcript</p>
    </header>
    <QuestionThread id="single-meeting-question" scope={`meeting:${sessionId}`} sourceIds={[sessionId]} {hasApiKey} {onAsk} {onOpenSettings} {starters} />
  </div>
</aside>
