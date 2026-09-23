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
  const starters = $derived([
    'Summarize the main ideas.',
    topic?.trim() && topic.length <= 100 ? `Explain “${topic.trim()}”.` : 'What details should I remember?'
  ]);
</script>

<section class="meeting-question" aria-labelledby="meeting-question-title">
  <header>
    <h2 id="meeting-question-title">Ask this meeting</h2>
    <p>From your notes and transcript</p>
  </header>
  <QuestionThread id="single-meeting-question" scope={`meeting:${sessionId}`} sourceIds={[sessionId]} {hasApiKey} {onAsk} {onOpenSettings} {starters} />
</section>

<style>
  .meeting-question { max-width: 720px; padding-block: 26px 10px; border-top: 1px solid var(--line); }
  header { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  h2 { margin: 0; font-family: Georgia, ui-serif, serif; font-size: 23px; font-weight: 400; letter-spacing: -.015em; line-height: 1.3; }
  header p { margin: 0; color: var(--muted-text); font-size: 12px; line-height: 1.5; }
  @media (max-width: 680px) { header { align-items: flex-start; flex-direction: column; gap: 5px; } }
</style>
