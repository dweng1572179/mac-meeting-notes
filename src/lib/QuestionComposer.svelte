<script lang="ts">
  import { questionShortcut } from './platform';

  let { id, question, asking, hasApiKey, onAsk, onInput, placeholder = 'Ask a question…' }: {
    id: string;
    question: string;
    asking: boolean;
    hasApiKey: boolean;
    onAsk: () => Promise<void>;
    onInput: (question: string) => void;
    placeholder?: string;
  } = $props();

  let textarea: HTMLTextAreaElement;
  export function focus() { textarea.focus(); }

  function autosize(node: HTMLTextAreaElement, _value: string) {
    let active = true;
    let animation: Animation | undefined;
    const fit = () => {
      if (!active) return;
      const previous = node.getBoundingClientRect().height;
      animation?.cancel();
      node.style.height = 'auto';
      const next = Math.min(node.scrollHeight, 156);
      node.style.height = `${next}px`;
      if (previous && previous !== next && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
        animation = node.animate([{ height: `${previous}px` }, { height: `${next}px` }], { duration: 140, easing: 'ease-out' });
      }
    };
    let width = 0;
    const observer = new ResizeObserver(() => {
      if (node.clientWidth !== width) { width = node.clientWidth; fit(); }
    });
    observer.observe(node);
    fit();
    return {
      update: () => queueMicrotask(fit),
      destroy: () => { active = false; observer.disconnect(); animation?.cancel(); }
    };
  }

  function submit(event: SubmitEvent | KeyboardEvent) {
    event.preventDefault();
    if (!asking && question.trim()) void onAsk();
  }
</script>

<form class="question-composer" onsubmit={submit} aria-busy={asking}>
  <label class="sr-only" for={id}>Question</label>
  <textarea bind:this={textarea} {id} value={question} use:autosize={question} oninput={(event) => onInput(event.currentTarget.value)}
    onkeydown={(event) => { if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && !event.isComposing) submit(event); }}
    rows="1" maxlength="2000" dir="auto" readonly={asking} {placeholder}
    aria-describedby={`${id}-hint`}></textarea>
  <button type="submit" disabled={asking || !question.trim()} aria-label={asking ? 'Reading meeting notes' : hasApiKey ? 'Ask question' : 'Add API key'} title={hasApiKey ? `Ask question (${questionShortcut()})` : 'Add API key'}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6" /></svg>
  </button>
</form>
<p class="composer-status" id={`${id}-hint`}>{asking ? 'Reading your notes…' : !hasApiKey ? 'Add your API key to ask.' : ''}<span>{asking ? '' : questionShortcut()}</span></p>

<style>
  .question-composer { display: flex; align-items: flex-end; gap: 8px; margin: 0; padding: 7px 8px 7px 12px; border: 1px solid var(--line); border-radius: 10px; background: #fcfbf7; transition: border-color 150ms ease; }
  .question-composer:focus-within { border-color: var(--accent-text); }
  .question-composer textarea { display: block; flex: 1; min-width: 0; width: 100%; min-height: 32px; max-height: 156px; padding: 6px 0; border: 0; border-radius: 0; outline: none; resize: none; overflow-y: auto; color: var(--ink); background: transparent; font-size: 14px; line-height: 20px; }
  .question-composer textarea:focus-visible { outline: none; }
  textarea::placeholder { color: var(--muted-text); }
  button { display: grid; place-items: center; flex: 0 0 32px; width: 32px; height: 32px; padding: 0; border: 0; border-radius: 8px; color: #fff; background: var(--accent-text); cursor: pointer; transition: background 150ms ease, color 150ms ease; }
  button:hover:not(:disabled) { background: #45520b; }
  button:disabled { color: var(--muted-text); background: var(--sidebar); cursor: default; }
  svg { width: 18px; height: 18px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .composer-status { display: flex; justify-content: space-between; min-height: 17px; margin: 6px 3px 0; color: var(--muted-text); font-size: 11px; line-height: 17px; }
  .composer-status span { margin-left: auto; }
  @media (prefers-reduced-motion: reduce) { .question-composer, button { transition: none; } }
</style>
