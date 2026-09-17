<script lang="ts">
  import { onMount } from 'svelte';
  import { saveApiKey, saveTranscriptionSettings } from './api';
  import { errorMessage } from './recovery';
  import type { TranscriptionSettings } from './types';

  let { hasApiKey, settings, onSaved, onSettingsSaved, onClose }: {
    hasApiKey: boolean; settings: TranscriptionSettings; onSaved: () => void;
    onSettingsSaved: (settings: TranscriptionSettings) => void; onClose: () => void;
  } = $props();
  const initial = <T,>(read: () => T) => read();
  let language = $state(initial(() => settings.language));
  let vocabulary = $state(initial(() => settings.vocabulary));
  let model = $state(initial(() => settings.model));
  let apiKey = $state('');
  let savingKey = $state(false);
  let savingPreferences = $state(false);
  let keyStatus = $state('');
  let keyError = $state(false);
  let preferenceStatus = $state('');
  let preferenceError = $state(false);
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let dirty = $derived(language !== settings.language || vocabulary !== settings.vocabulary || model !== settings.model);
  const languageNames = new Intl.DisplayNames(['en'], { type: 'language' });
  const languages = 'af ar hy az be bs bg ca zh hr cs da nl en et fi fr gl de el he hi hu is id it ja kn kk ko lv lt mk ms mr mi ne no fa pl pt ro ru sr sk sl es sw sv tl ta th tr uk ur vi cy'.split(' ')
    .map((code) => ({ code, name: languageNames.of(code) ?? code })).sort((a, b) => a.name.localeCompare(b.name));

  onMount(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.showModal();
    if (!hasApiKey) input.focus();
    return () => { if (dialog.open) dialog.close(); previousFocus?.focus(); };
  });

  async function saveKey() {
    if (savingKey || !apiKey.trim()) return;
    savingKey = true;
    keyError = false;
    keyStatus = 'Checking key…';
    try {
      await saveApiKey(apiKey);
      apiKey = '';
      onSaved();
      keyStatus = 'API key saved in your Mac Keychain. You can close settings and continue.';
    } catch (error) {
      keyError = true;
      keyStatus = errorMessage(error, 'The API key could not be saved. Try again.');
    } finally { savingKey = false; }
  }

  async function savePreferences() {
    if (savingPreferences) return;
    savingPreferences = true;
    preferenceError = false;
    preferenceStatus = 'Saving…';
    try {
      const saved = await saveTranscriptionSettings({ language, vocabulary, model });
      language = saved.language;
      vocabulary = saved.vocabulary;
      model = saved.model;
      onSettingsSaved(saved);
      preferenceStatus = 'Saved for new recordings and explicit no-speech retries.';
    } catch (error) {
      preferenceError = true;
      preferenceStatus = errorMessage(error, 'Preferences could not be saved. Your changes are still here.');
    } finally { savingPreferences = false; }
  }
</script>

<dialog bind:this={dialog} class="settings-dialog preferences-dialog" aria-labelledby="settings-title"
  oncancel={(event) => { if (savingKey || savingPreferences) event.preventDefault(); }} onclose={onClose}>
  <button class="dialog-close" type="button" aria-label="Close settings" disabled={savingKey || savingPreferences} onclick={() => dialog.close()}>×</button>
  <h2 id="settings-title">Settings</h2>
  <p class="settings-copy">Your defaults for clearer meeting transcripts. Original notes always stay yours.</p>

  <form class="preferences-form" onsubmit={(event) => { event.preventDefault(); void savePreferences(); }}>
    <fieldset disabled={savingPreferences}>
      <legend>Transcription</legend>
      <label for="spoken-language">Spoken language</label>
      <select id="spoken-language" bind:value={language} aria-describedby="language-help">
        <option value="">Automatic detection</option>
        {#each languages as item}<option value={item.code}>{item.name}</option>{/each}
      </select>
      <p id="language-help" class="field-help">Choose the spoken language to guide recognition, including regional accents. Use automatic detection for mixed-language meetings.</p>
      <label for="transcription-model">Accuracy and cost</label>
      <select id="transcription-model" bind:value={model} aria-describedby="model-help">
        <option value="gpt-4o-mini-transcribe">Standard · lower cost</option>
        <option value="gpt-4o-transcribe">Higher accuracy · higher cost</option>
      </select>
      <p id="model-help" class="field-help">Higher accuracy can help with difficult speech and terminology. OpenAI bills your API account; recognition is not guaranteed.</p>
      <label for="vocabulary">Names and vocabulary</label>
      <textarea id="vocabulary" bind:value={vocabulary} rows="3" maxlength="4000" aria-describedby="vocabulary-help" placeholder="Darryl Weng, São Paulo, cap rate, amortization"></textarea>
      <p id="vocabulary-help" class="field-help">Add names, places, and technical terms, separated by commas or lines. These are recognition hints, not voice training. {[...vocabulary].length.toLocaleString()} / 2,000 characters.</p>
      <div class="preferences-actions">
        <span class="field-help">{dirty ? 'Unsaved changes' : 'Saved defaults'}</span>
        <button class="save-button" type="submit" disabled={!dirty || [...vocabulary].length > 2000}>{savingPreferences ? 'Saving…' : 'Save defaults'}</button>
      </div>
    </fieldset>
    {#if preferenceStatus}<p class:error={preferenceError} class="settings-status" role={preferenceError ? 'alert' : 'status'}>{preferenceStatus}</p>{/if}
  </form>

  <form class="key-form" onsubmit={(event) => { event.preventDefault(); void saveKey(); }}>
    <h3>OpenAI API key</h3>
    <p class="field-help">{hasApiKey ? 'A key is saved in your Mac Keychain. Enter a new key only to replace it.' : 'Add a key to transcribe recordings and create enhanced notes.'}</p>
    <label class="sr-only" for="api-key">API key</label>
    <input bind:this={input} bind:value={apiKey} id="api-key" name="api-key" type="password" autocomplete="off" spellcheck="false" placeholder="sk-…" disabled={savingKey} />
    <div class="preferences-actions">
      <p class="field-help">Audio, vocabulary, transcripts, and notes go directly to OpenAI when processed. Your key stays in Keychain.</p>
      <button class="save-button" type="submit" disabled={savingKey || !apiKey.trim()}>{savingKey ? 'Checking…' : hasApiKey ? 'Replace key' : 'Save key'}</button>
    </div>
    {#if keyStatus}<p class:error={keyError} class="settings-status" role={keyError ? 'alert' : 'status'}>{keyStatus}</p>{/if}
  </form>
</dialog>

<style>
  .preferences-dialog { width: min(620px, calc(100vw - 32px)); max-height: calc(100dvh - 48px); overflow-y: auto; }
  .preferences-form { margin-top: 24px; }
  fieldset { min-width: 0; margin: 0; padding: 0; border: 0; }
  legend, h3 { margin: 0 0 16px; font-size: 16px; font-weight: 600; }
  label { display: block; margin: 18px 0 7px; font-size: 14px; }
  select, textarea { width: 100%; padding: 10px 12px; border: 1px solid var(--line); border-radius: 6px; color: var(--ink); background: var(--paper); font: inherit; font-size: 14px; }
  textarea { resize: vertical; min-height: 88px; }
  select:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .field-help { margin: 7px 0 0; color: var(--muted-text); font-size: 13px; line-height: 1.5; }
  .preferences-actions { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-top: 16px; }
  .preferences-actions .field-help { margin: 0; }
  .save-button { flex-shrink: 0; white-space: nowrap; }
  .key-form { margin-top: 28px; padding-top: 24px; border-top: 1px solid var(--line); }
  .key-form input { margin-top: 12px; }
  .settings-status { color: var(--accent-text); font-size: 13px; line-height: 1.5; overflow-wrap: anywhere; }
  .settings-status.error { color: var(--danger); }
  @media (max-width: 520px) { .preferences-actions { align-items: stretch; flex-direction: column; gap: 12px; } }
</style>
