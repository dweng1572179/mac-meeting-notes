<script lang="ts">
  import { markdownInlines } from './markdown';
  let { text }: { text: string } = $props();
</script>

<span>{#each markdownInlines(text) as part}{#if part.kind === 'strong'}<strong>{part.text}</strong>{:else if part.kind === 'code'}<code>{part.text}</code>{:else}{part.text}{/if}{/each}</span>

<style>
  strong { font-weight: 650; }
  code { padding: 1px 4px; border-radius: 3px; background: var(--sidebar); font-size: .9em; overflow-wrap: anywhere; }
</style>
