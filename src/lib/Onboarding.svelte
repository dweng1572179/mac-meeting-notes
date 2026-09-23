<script lang="ts">
  import { currentPlatform } from './platform';

  let { hasApiKey, keyAccessError, onSetupAI, onStart, onDismiss }: {
    hasApiKey: boolean;
    keyAccessError: string | null;
    onSetupAI: () => void;
    onStart: () => void;
    onDismiss: () => void;
  } = $props();
  const device = currentPlatform() === 'mac' ? 'Mac' : 'computer';
</script>

<section class="welcome" aria-labelledby="welcome-title">
  <h1 id="welcome-title">A clear record of<br />every conversation.</h1>
  <p class="welcome-intro">Keep your notes on this {device}. Add recording and AI when you’re ready. No Meeting Notes account needed.</p>

  <ol class="welcome-steps">
    <li><strong>Start with a note</strong><p>Write a title, add context, and type what matters. Your notes save automatically on this computer.</p></li>
    <li><strong>Add AI on your terms</strong><p>Your OpenAI API key enables transcription and answers. Audio and relevant text go to OpenAI for processing; API usage is billed to your OpenAI account.</p></li>
    <li><strong>Record when you’re ready</strong><p>You start and stop every recording. Allow microphone and system audio access when prompted, and let others know you’re recording.</p></li>
  </ol>

  {#if keyAccessError}<p class="welcome-error" role="alert">{keyAccessError} You can still write and open local notes.</p>{/if}
  <div class="welcome-actions">
    {#if hasApiKey}
      <button class="save-button" type="button" onclick={onStart}>Create your first note</button>
      <span>AI key saved on this {device}</span>
    {:else}
      <button class="save-button" type="button" onclick={onSetupAI}>Set up recording &amp; AI</button>
      <button class="text-button" type="button" onclick={onStart}>Start with typed notes</button>
    {/if}
  </div>
  <footer><button class="text-button" type="button" onclick={onDismiss}>Explore the library</button><span>You can revisit this guide in Settings.</span></footer>
</section>

<style>
  .welcome { max-width: 720px; margin: 0 auto; padding: 0 0 24px; }
  h1 { margin: 0; font-family: Georgia, ui-serif, serif; font-size: clamp(32px, 3.8vw, 46px); font-weight: 400; line-height: 1.12; letter-spacing: -0.025em; text-wrap: balance; }
  .welcome-intro { max-width: 55ch; margin: 20px 0 0; font-size: 16px; line-height: 1.65; color: var(--muted-text); }
  .welcome-steps { list-style: none; padding: 0; margin: 30px 0; display: grid; gap: 20px; }
  .welcome-steps li { display: grid; grid-template-columns: 170px 1fr; gap: 24px; }
  strong { font-size: 14px; font-weight: 600; line-height: 1.6; }
  .welcome-steps p { margin: 0; max-width: 55ch; font-size: 14px; line-height: 1.6; color: var(--muted-text); }
  .welcome-actions, footer { display: flex; flex-wrap: wrap; align-items: center; gap: 16px; }
  .welcome-actions span, footer span { font-size: 12px; color: var(--muted-text); }
  .text-button { padding: 8px 0; border: 0; color: var(--ink); background: transparent; font-size: 14px; text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
  footer { margin-top: 30px; padding-top: 14px; border-top: 1px solid var(--line); }
  footer .text-button { font-size: 12px; }
  .welcome-error { color: var(--danger); font-size: 14px; line-height: 1.5; }
  @media (max-width: 980px) { .welcome-steps li { grid-template-columns: 1fr; gap: 3px; } .welcome-steps { margin: 24px 0; gap: 16px; } }
</style>
