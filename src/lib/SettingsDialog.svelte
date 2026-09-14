<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { saveApiKey } from './api';

  let { hasApiKey, onClose }: { hasApiKey: boolean; onClose: () => void } = $props();
  let apiKey = $state('');
  let saving = $state(false);
  let status = $state('');
  let input: HTMLInputElement;

  onMount(() => {
    status = hasApiKey ? 'An API key is saved in your Mac Keychain.' : 'No API key saved.';
    tick().then(() => input.focus());
  });

  async function save() {
    if (!apiKey.trim()) {
      status = 'Enter an API key before saving.';
      return;
    }

    saving = true;
    status = 'Checking key…';
    try {
      await saveApiKey(apiKey);
      apiKey = '';
      status = 'API key saved in your Mac Keychain.';
    } catch (error) {
      status = `${error instanceof Error ? error.message : 'The key could not be saved.'} Check the key and try again.`;
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={(event) => event.key === 'Escape' && onClose()} />

<div class="dialog-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <dialog open class="settings-dialog" aria-modal="true" aria-labelledby="settings-title">
    <button class="dialog-close" type="button" aria-label="Close settings" onclick={onClose}>×</button>
    <h2 id="settings-title">OpenAI settings</h2>
    <p class="settings-copy">
      Meeting audio, transcripts, and notes go directly to OpenAI for transcription and enrichment.
      Your API key is stored in your Mac Keychain and never written to your notes.
    </p>

    <form onsubmit={(event) => { event.preventDefault(); save(); }}>
      <label for="api-key">API key</label>
      <input
        bind:this={input}
        bind:value={apiKey}
        id="api-key"
        name="api-key"
        type="password"
        autocomplete="off"
        spellcheck="false"
        placeholder="sk-…"
      />
      <div class="settings-footer">
        <p class="status" aria-live="polite">{status}</p>
        <button class="save-button" type="submit" disabled={saving}>{saving ? 'Saving…' : 'Save key'}</button>
      </div>
    </form>
  </dialog>
</div>
