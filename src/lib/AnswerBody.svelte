<script lang="ts">
  import { parseAnswerMarkdown } from './markdown';
  import InlineMarkdown from './InlineMarkdown.svelte';
  let { text }: { text: string } = $props();
</script>

<div class="answer-copy" dir="auto">
  {#each parseAnswerMarkdown(text) as block}
    {#if block.kind === 'list'}
      {#if block.ordered}
        <ol start={block.start}>{#each block.items as item}<li><InlineMarkdown text={item} /></li>{/each}</ol>
      {:else}
        <ul>{#each block.items as item}<li><InlineMarkdown text={item} /></li>{/each}</ul>
      {/if}
    {:else if block.kind === 'heading'}
      <h3><InlineMarkdown text={block.text} /></h3>
    {:else}
      <p><InlineMarkdown text={block.text} /></p>
    {/if}
  {/each}
</div>

<style>
  .answer-copy { max-width: 68ch; color: var(--ink); font-size: 14px; line-height: 1.75; overflow-wrap: anywhere; }
  h3 { margin: 20px 0 7px; font-size: 14px; font-weight: 650; line-height: 1.5; }
  p { margin: 0 0 10px; }
  ul, ol { margin: 8px 0 14px; padding-inline-start: 21px; }
  li { padding-inline-start: 3px; }
  li + li { margin-top: 5px; }
  li::marker { color: var(--muted-text); }
  .answer-copy > :first-child { margin-top: 0; }
  .answer-copy > :last-child { margin-bottom: 0; }
</style>
