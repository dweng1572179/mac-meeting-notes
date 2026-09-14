<script lang="ts">
  import { onMount } from 'svelte';
  import { saveApiKey } from './api';

  let {
    hasApiKey,
    onSaved,
    onClose
  }: { hasApiKey: boolean; onSaved: () => void; onClose: () => void } = $props();
  let apiKey = $state('');
  let saving = $state(false);
  let status = $state('');
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;

  onMount(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    status = hasApiKey ? 'An API key is saved in your Mac Keychain.' : 'No API key saved.';
    dialog.showModal();
    input.focus();

    return () => {
      if (dialog.open) dialog.close();
      previousFocus?.focus();
    };
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
      onSaved();
      status = 'API key saved in your Mac Keychain.';
    } catch (error) {
      status = `${error instanceof Error ? error.message : 'The key could not be saved.'} Check the key and try again.`;
    } finally {
      saving = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="settings-dialog"
  aria-labelledby="settings-title"
  oncancel={(event) => { event.preventDefault(); dialog.close(); }}
  onclose={onClose}
>
  <button class="dialog-close" type="button" aria-label="Close settings" onclick={() => dialog.close()}>×</button>
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
