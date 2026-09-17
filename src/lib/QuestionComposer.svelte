<script lang="ts">
  let { id, question = $bindable(''), asking, hasApiKey, onAsk, onInput }: {
    id: string;
    question: string;
    asking: boolean;
    hasApiKey: boolean;
    onAsk: () => Promise<void>;
    onInput: () => void;
  } = $props();

  function autosize(node: HTMLTextAreaElement, _value: string) {
    const fit = () => { node.style.height = '0px'; node.style.height = `${Math.min(node.scrollHeight, 156)}px`; };
    let width = 0;
    const observer = new ResizeObserver(() => {
      if (node.clientWidth !== width) { width = node.clientWidth; fit(); }
    });
    observer.observe(node);
    fit();
    return { update: fit, destroy: () => observer.disconnect() };
  }

  function submit(event: SubmitEvent | KeyboardEvent) {
    event.preventDefault();
    if (!asking && question.trim()) void onAsk();
  }
</script>

<form class="question-composer" onsubmit={submit} aria-busy={asking}>
  <label class="sr-only" for={id}>Question</label>
  <textarea {id} bind:value={question} use:autosize={question} oninput={onInput}
    onkeydown={(event) => { if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && !event.isComposing) submit(event); }}
    rows="1" maxlength="2000" dir="auto" readonly={asking} placeholder="Ask a question…"
    aria-describedby={`${id}-hint`}></textarea>
  <button type="submit" disabled={asking || !question.trim()} aria-label={asking ? 'Reading meeting notes' : hasApiKey ? 'Ask question' : 'Add API key'} title={hasApiKey ? 'Ask question (⌘ Enter)' : 'Add API key'}>
    {#if asking}<span class="pending" aria-hidden="true">···</span>
    {:else}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6" /></svg>{/if}
  </button>
</form>
<p class="composer-status" id={`${id}-hint`} aria-live="polite">{asking ? 'Reading your notes…' : !hasApiKey ? 'Add your API key to ask.' : ''}<span>{asking ? '' : '⌘ Enter'}</span></p>

<style>
  .question-composer { display: flex; align-items: flex-end; gap: 12px; margin: 0; padding: 9px 10px 9px 16px; border: 1px solid var(--line); border-radius: 16px; background: #fcfbf7; transition: border-color 180ms ease; }
  .question-composer:focus-within { border-color: var(--accent-text); outline: 1px solid var(--accent-text); outline-offset: 0; }
  .question-composer textarea { display: block; flex: 1; min-width: 0; width: 100%; min-height: 32px; max-height: 156px; padding: 6px 0; border: 0; border-radius: 0; outline: none; resize: none; overflow-y: auto; color: var(--ink); background: transparent; font-size: 14px; line-height: 20px; }
  .question-composer textarea:focus-visible { outline: none; }
  textarea::placeholder { color: var(--muted-text); }
  button { display: grid; place-items: center; flex: 0 0 32px; width: 32px; height: 32px; padding: 0; border: 0; border-radius: 50%; color: var(--paper); background: var(--ink); cursor: pointer; transition: background 160ms ease, color 160ms ease, transform 160ms ease; }
  button:hover:not(:disabled) { background: var(--accent-text); }
  button:active:not(:disabled) { transform: scale(.94); }
  button:disabled { color: var(--muted-text); background: var(--sidebar); cursor: default; }
  svg { width: 18px; height: 18px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .pending { font-size: 20px; line-height: 1; }
  .composer-status { display: flex; justify-content: space-between; min-height: 17px; margin: 6px 3px 0; color: var(--muted-text); font-size: 11px; line-height: 17px; }
  .composer-status span { margin-left: auto; }
  @media (prefers-reduced-motion: reduce) { .question-composer, button { transition: none; } }
</style>
