<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { hasApiKey as checkApiKey, openSettingsDestination, removeApiKey, saveApiKey, saveTranscriptionSettings, type SettingsDestination } from './api';
  import { currentPlatform } from './platform';
  import { errorMessage } from './recovery';
  import type { TranscriptionSettings } from './types';

  let { hasApiKey, keyAccessError = null, dataDirectory = '', busy = false, settings, initialSection = 'recording', onKeyChanged, onSettingsSaved, onShowSetup, onClose }: {
    hasApiKey: boolean; keyAccessError?: string | null; dataDirectory?: string; busy?: boolean;
    settings: TranscriptionSettings; initialSection?: 'ai' | 'recording'; onKeyChanged: (saved: boolean) => void;
    onSettingsSaved: (settings: TranscriptionSettings) => void; onShowSetup: () => void; onClose: () => void;
  } = $props();
  const initial = <T,>(read: () => T) => read();
  const sections = [{ id: 'ai', label: 'AI & API key' }, { id: 'recording', label: 'Recording' }, { id: 'privacy', label: 'Privacy & data' }, { id: 'connections', label: 'Connections' }] as const;
  let section = $state<(typeof sections)[number]['id']>(initial(() => initialSection));
  let language = $state(initial(() => settings.language));
  let vocabulary = $state(initial(() => settings.vocabulary));
  let model = $state(initial(() => settings.model));
  let apiKey = $state('');
  let savingKey = $state(false);
  let savingPreferences = $state(false);
  let confirmingRemoval = $state(false);
  let keyStatus = $state('');
  let keyError = $state(false);
  let preferenceStatus = $state('');
  let preferenceError = $state(false);
  let actionError = $state('');
  let dialog: HTMLDialogElement;
  let discardDialog: HTMLDialogElement;
  let showSetupAfterClose = false;
  let input = $state<HTMLInputElement>();
  let dirty = $derived(language !== settings.language || vocabulary !== settings.vocabulary || model !== settings.model);
  let working = $derived(savingKey || savingPreferences);
  const platform = currentPlatform();
  const credentialStore = platform === 'mac' ? 'macOS Keychain' : platform === 'windows' ? 'Windows Credential Manager' : 'your system credential store';
  const languageNames = new Intl.DisplayNames(['en'], { type: 'language' });
  const languages = 'af ar hy az be bs bg ca zh hr cs da nl en et fi fr gl de el he hi hu is id it ja kn kk ko lv lt mk ms mr mi ne no fa pl pt ro ru sr sk sl es sw sv tl ta th tr uk ur vi cy'.split(' ')
    .map((code) => ({ code, name: languageNames.of(code) ?? code })).sort((a, b) => a.name.localeCompare(b.name));

  onMount(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.showModal();
    if (section === 'ai' && !hasApiKey) input?.focus();
    return () => { if (discardDialog.open) discardDialog.close(); if (dialog.open) dialog.close(); previousFocus?.focus(); };
  });

  function finishClose(showSetup = false) {
    discardDialog.close();
    dialog.close();
    if (showSetup) onShowSetup();
  }

  function requestClose(showSetup = false) {
    if (working) return;
    if (dirty || apiKey) {
      showSetupAfterClose = showSetup;
      discardDialog.showModal();
    } else finishClose(showSetup);
  }

  async function selectSection(next: typeof section) {
    section = next;
    actionError = '';
    confirmingRemoval = false;
    await tick();
    dialog.querySelector<HTMLElement>('.settings-content h3')?.focus();
  }

  async function openDestination(destination: SettingsDestination) {
    actionError = '';
    try { await openSettingsDestination(destination); }
    catch (error) { actionError = errorMessage(error, 'This destination could not be opened. Try again.'); }
  }

  async function saveKey() {
    if (savingKey || !apiKey.trim()) return;
    savingKey = true;
    keyError = false;
    keyStatus = 'Checking key with OpenAI…';
    try {
      await saveApiKey(apiKey);
      apiKey = '';
      onKeyChanged(true);
      keyStatus = 'Key saved. Recording and AI are ready to try; billing and model access are managed in your OpenAI account.';
    } catch (error) {
      keyError = true;
      keyStatus = errorMessage(error, 'The API key could not be saved. Your existing key has not been replaced.');
    } finally { savingKey = false; }
  }

  async function removeKey() {
    if (savingKey || busy) return;
    savingKey = true;
    keyError = false;
    keyStatus = '';
    try {
      await removeApiKey();
      apiKey = '';
      onKeyChanged(false);
      confirmingRemoval = false;
      keyStatus = 'Saved key removed from this computer. Your notes are still here.';
    } catch (error) {
      keyError = true;
      keyStatus = errorMessage(error, 'The key could not be removed. Try again.');
    } finally { savingKey = false; }
  }

  async function retryKeyAccess() {
    if (savingKey) return;
    savingKey = true;
    keyError = false;
    try {
      const saved = await checkApiKey();
      onKeyChanged(saved);
      keyStatus = saved ? 'Your saved key is available again.' : 'Credential store opened. No API key is saved.';
    } catch (error) {
      keyError = true;
      keyStatus = errorMessage(error, 'Your credential store is still unavailable. Local notes remain available.');
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
  oncancel={(event) => { event.preventDefault(); requestClose(); }} onclose={onClose}>
  <header class="settings-heading">
    <h2 id="settings-title">Settings</h2>
    <p class="settings-copy">Local notes. AI when you choose.</p>
    <button class="dialog-close" type="button" aria-label="Close settings" disabled={working} onclick={() => requestClose()}>×</button>
  </header>
  <div class="settings-layout">
    <nav aria-label="Settings sections">
      {#each sections as item}<button type="button" class:active={section === item.id} aria-current={section === item.id ? 'page' : undefined} onclick={() => selectSection(item.id)}>{item.label}{#if item.id === 'recording' && dirty}<span class="sr-only"> — unsaved changes</span>{/if}</button>{/each}
      <button class="setup-link" type="button" disabled={working} onclick={() => requestClose(true)}>Getting started</button>
    </nav>

    <div class="settings-content">
      {#if section === 'ai'}
        <h3 tabindex="-1">Your AI connection</h3>
        <p class="section-intro">Use your own OpenAI API key for transcription, notes, and answers. Audio and relevant meeting text are sent to OpenAI when you use these features. Typed notes work without a key.</p>
        <p class="connection-status">{keyAccessError ? 'Credential store unavailable' : hasApiKey ? 'API key saved on this computer' : 'No API key saved'}</p>
        {#if keyAccessError}<div class="access-error" role="alert"><p>{keyAccessError}</p><button type="button" class="secondary-button" disabled={savingKey} onclick={retryKeyAccess}>Check again</button></div>{/if}
        <form onsubmit={(event) => { event.preventDefault(); void saveKey(); }}>
          <label for="api-key">{hasApiKey ? 'Replace API key' : 'OpenAI API key'}</label>
          <input bind:this={input} bind:value={apiKey} id="api-key" name="api-key" type="password" autocomplete="off" autocapitalize="none" spellcheck="false" maxlength="4096" placeholder="sk-…" disabled={savingKey} aria-describedby="key-help" />
          <p id="key-help" class="field-help">Stored in {credentialStore}. Saving checks the key with OpenAI; the saved key is never displayed here.</p>
          <div class="preferences-actions"><button class="save-button" type="submit" disabled={savingKey || !apiKey.trim()}>{savingKey ? 'Working…' : hasApiKey ? 'Replace key' : 'Check & save key'}</button><button class="text-button" type="button" onclick={() => openDestination('api-keys')}>Create or manage keys</button></div>
        </form>
        {#if keyStatus}<p class:error={keyError} class="settings-status" role={keyError ? 'alert' : 'status'}>{keyStatus}</p>{/if}
        <div class="settings-group">
          <h4>Usage &amp; billing</h4>
          <p class="field-help">OpenAI bills your API account directly. A ChatGPT subscription does not include API usage. Microphone and system audio are processed as separate tracks.</p>
          <button class="text-button" type="button" onclick={() => openDestination('billing')}>Open API billing</button>
        </div>
        {#if hasApiKey || keyAccessError}
          <div class="settings-group">
            {#if confirmingRemoval}
              <h4>Remove the saved key?</h4>
              <p class="field-help">This removes it from this computer, not from OpenAI. Your notes stay here. Requests already sent may finish. Revoke the key in your OpenAI account to invalidate it there.</p>
              <div class="preferences-actions"><button class="danger-button" type="button" disabled={savingKey || busy} onclick={removeKey}>Remove from this computer</button><button class="secondary-button" type="button" disabled={savingKey} onclick={() => (confirmingRemoval = false)}>Keep key</button></div>
            {:else}<button class="text-button danger-text" type="button" disabled={savingKey || busy} onclick={() => (confirmingRemoval = true)}>Remove saved key…</button>{/if}
            {#if busy}<p class="field-help">Stop recording and wait for processing to finish before removing the key.</p>{/if}
          </div>
        {/if}
      {:else if section === 'recording'}
        <h3 tabindex="-1">Recording defaults</h3>
        <p class="section-intro">Transcription runs during your meeting. Notes are prepared once you stop.</p>
        <form onsubmit={(event) => { event.preventDefault(); void savePreferences(); }}>
          <fieldset disabled={savingPreferences}>
            <legend class="sr-only">Transcription defaults</legend>
            <label for="spoken-language">Spoken language</label>
            <select id="spoken-language" bind:value={language} aria-describedby="language-help"><option value="">Automatic detection</option>{#each languages as item}<option value={item.code}>{item.name}</option>{/each}</select>
            <p id="language-help" class="field-help">Choose the spoken language to guide recognition. Use automatic detection for mixed-language meetings.</p>
            <label for="transcription-model">Transcript style</label>
            <select id="transcription-model" bind:value={model} aria-describedby="model-help"><option value="gpt-4o-transcribe-diarize">Speaker detection</option><option value="gpt-4o-mini-transcribe">Text only · lowest cost</option><option value="gpt-4o-transcribe">Text only · higher accuracy</option></select>
            <p id="model-help" class="field-help">{model === 'gpt-4o-transcribe-diarize' ? 'Separates voices into speaker turns. Labels are local to each audio section; names need supporting evidence. Costs more than text-only mini.' : 'Keeps a plain transcript with language and vocabulary hints. Speaker identities are not detected.'}</p>
            <label for="vocabulary">Names and vocabulary</label>
            <textarea id="vocabulary" bind:value={vocabulary} disabled={model === 'gpt-4o-transcribe-diarize'} rows="3" maxlength="4000" aria-describedby="vocabulary-help" placeholder="Names, places, or technical terms"></textarea>
            <p id="vocabulary-help" class="field-help">{model === 'gpt-4o-transcribe-diarize' ? 'Vocabulary hints are unavailable with speaker detection. Your vocabulary is kept for text-only mode.' : 'Separate terms with commas or lines. Hints guide recognition; they do not train your voice.'} {[...vocabulary].length.toLocaleString()} / 2,000 characters.</p>
            <div class="preferences-actions"><span class="field-help">{dirty ? 'Unsaved changes' : 'Saved defaults'}</span><button class="save-button" type="submit" disabled={!dirty || [...vocabulary].length > 2000}>{savingPreferences ? 'Saving…' : 'Save defaults'}</button></div>
          </fieldset>
          {#if preferenceStatus}<p class:error={preferenceError} class="settings-status" role={preferenceError ? 'alert' : 'status'}>{preferenceStatus}</p>{/if}
        </form>
        <div class="settings-group">
          <h4>Audio permissions</h4>
          <p class="field-help">{platform === 'mac' ? 'Allow Meeting Notes in System Settings → Privacy & Security → Microphone and Screen & System Audio Recording. macOS may ask you to reopen the app after a change.' : platform === 'windows' ? 'Allow microphone access for desktop apps in Windows Settings. System audio uses your active playback device; choose the correct input and output in Sound settings.' : 'Allow microphone and system audio access in your operating system settings.'} Access is checked when you start recording.</p>
          <div class="help-actions"><button class="secondary-button" type="button" onclick={() => openDestination('microphone')}>Microphone settings</button><button class="secondary-button" type="button" onclick={() => openDestination('system-audio')}>{platform === 'windows' ? 'Sound settings' : 'System audio settings'}</button></div>
          <p class="field-help">Opening settings does not grant permission. Start recording only when everyone is aware; check both audio sources in the recording status.</p>
        </div>
      {:else if section === 'privacy'}
        <h3 tabindex="-1">Your data, on this computer</h3>
        <p class="section-intro">Meeting Notes has no account or cloud sync. Local storage and cloud AI processing are separate.</p>
        <dl class="privacy-list">
          <div><dt>Saved locally</dt><dd>Notes, transcripts, meeting details, and AI conversations. Notes and transcripts are ordinary local files; the app does not encrypt them. Use your operating system account and disk encryption to protect this computer.</dd></div>
          <div><dt>Sent to OpenAI</dt><dd>Audio and recognition hints for transcription; relevant notes, transcripts, meeting context, questions, and recent chat for AI responses. Typed edits save locally. Recording, retrying processing, updating notes with AI, and asking questions send the content needed for that action.</dd></div>
          <div><dt>Retained audio</dt><dd>Successfully transcribed recording sections are removed locally. Unfinished or failed audio can remain for recovery. Deleting a meeting also removes its retained audio and related saved AI conversations.</dd></div>
          <div><dt>Credentials</dt><dd>Your API key is held in {credentialStore}, separately from your notes. Removing it here does not revoke it with OpenAI.</dd></div>
        </dl>
        <div class="settings-group">
          <h4>Files &amp; backups</h4>
          {#if dataDirectory}<p class="data-path">{dataDirectory}</p>{/if}
          <p class="field-help">Export individual notes or transcripts from a meeting. For a consistent backup of the local files, stop recording, wait for processing, then quit the app before copying this folder. AI chats use separate app browser storage; a copy of this folder is not a complete chat backup.</p>
          <button class="secondary-button" type="button" onclick={() => openDestination('data-folder')}>Open data folder</button>
          <p class="field-help">Removing the app alone may leave its local data. Meeting deletion is permanent in this version; export anything you need first.</p>
        </div>
      {:else}
        <h3 tabindex="-1">Optional connections</h3>
        <p class="section-intro">Your local workspace does not need a Google or Microsoft account.</p>
        <dl class="privacy-list">
          <div><dt>Google &amp; Microsoft calendars</dt><dd>Calendar connections are not available in this version. You can add a meeting title, participants, and context directly in any note.</dd></div>
          <div><dt>Email</dt><dd>Meeting Notes does not read your Gmail or Outlook inbox.</dd></div>
          <div><dt>AI processing</dt><dd>Your OpenAI API key is a separate connection. Manage it under AI &amp; API key.</dd></div>
        </dl>
        <button class="secondary-button" type="button" onclick={() => selectSection('ai')}>Manage AI connection</button>
      {/if}
      {#if actionError}<p class="settings-status error" role="alert">{actionError}</p>{/if}
    </div>
  </div>
</dialog>

<dialog bind:this={discardDialog} class="settings-dialog discard-dialog" aria-labelledby="discard-settings-title" aria-describedby="discard-settings-help">
  <h2 id="discard-settings-title">Discard unsaved changes?</h2>
  <p id="discard-settings-help" class="settings-copy">Your saved settings and API key will stay unchanged.</p>
  <div class="preferences-actions">
    <button class="secondary-button" type="button" onclick={() => discardDialog.close()}>Keep editing</button>
    <button class="danger-button" type="button" onclick={() => finishClose(showSetupAfterClose)}>Discard changes</button>
  </div>
</dialog>

<style>
  .preferences-dialog { width: min(800px, calc(100vw - 32px)); max-width: calc(100vw - 32px); height: min(660px, calc(100dvh - 40px)); max-height: calc(100dvh - 40px); padding: 0; overflow: hidden; color: var(--ink); display: none; }
  .preferences-dialog[open] { display: flex; flex-direction: column; }
  .settings-heading { padding: 24px 28px 18px; border-bottom: 1px solid var(--line); }
  .settings-heading .settings-copy { margin: 7px 0 0; }
  .settings-layout { display: grid; grid-template-columns: 168px minmax(0, 1fr); min-height: 0; flex: 1; }
  nav { padding: 16px 10px; border-right: 1px solid var(--line); background: var(--sidebar); overflow-y: auto; }
  nav button { display: block; width: 100%; border: 0; border-radius: 6px; padding: 10px; background: transparent; text-align: left; font-size: 13px; cursor: pointer; }
  nav button:hover, nav button.active { background: rgba(41, 41, 33, 0.07); }
  nav button.active { font-weight: 600; }
  nav .setup-link { margin-top: 24px; color: var(--muted-text); }
  .settings-content { min-width: 0; padding: 24px 28px; overflow-y: auto; overscroll-behavior: contain; }
  h3 { margin: 0 0 8px; font-size: 19px; font-weight: 600; }
  h3:focus { outline: none; }
  h4 { margin: 0; font-size: 14px; font-weight: 600; }
  .section-intro { margin: 0 0 20px; font-size: 14px; line-height: 1.6; color: var(--muted-text); }
  .connection-status { margin: 0 0 18px; font-size: 13px; font-weight: 600; }
  fieldset { min-width: 0; margin: 0; padding: 0; border: 0; }
  label { display: block; margin: 18px 0 7px; font-size: 14px; }
  select, textarea { width: 100%; padding: 10px 12px; border: 1px solid var(--line); border-radius: 6px; color: var(--ink); background: var(--paper); font: inherit; font-size: 14px; }
  textarea { resize: vertical; min-height: 80px; }
  .field-help { margin: 7px 0 0; color: var(--muted-text); font-size: 13px; line-height: 1.55; }
  .preferences-actions, .help-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; margin-top: 16px; }
  .help-actions { justify-content: flex-start; }
  .preferences-actions .field-help { margin: 0; }
  .save-button { white-space: nowrap; font-size: 13px; }
  .secondary-button, .danger-button { padding: 8px 12px; border: 1px solid var(--line); border-radius: 6px; background: transparent; font-size: 13px; cursor: pointer; }
  .text-button { padding: 8px 0; border: 0; color: var(--ink); background: transparent; font-size: 13px; text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
  .danger-button, .danger-text { color: var(--danger); }
  .danger-button { border-color: var(--danger); }
  button:disabled { opacity: 0.55; cursor: default; }
  .settings-group { margin-top: 24px; padding-top: 20px; border-top: 1px solid var(--line); }
  .settings-group > .secondary-button { margin-top: 14px; }
  .settings-status { color: var(--accent-text); font-size: 13px; line-height: 1.55; overflow-wrap: anywhere; }
  .settings-status.error, .access-error { color: var(--danger); }
  .access-error { font-size: 13px; line-height: 1.55; margin-bottom: 16px; }
  .privacy-list { margin: 0; }
  .privacy-list div + div { margin-top: 20px; }
  dt { font-size: 14px; font-weight: 600; }
  dd { margin: 6px 0 0; font-size: 13px; line-height: 1.6; color: var(--muted-text); }
  .data-path { margin: 10px 0; font-size: 12px; line-height: 1.5; overflow-wrap: anywhere; }
  @media (max-width: 600px) {
    .settings-layout { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
    nav { display: flex; gap: 4px; padding: 8px; border-right: 0; border-bottom: 1px solid var(--line); overflow-x: auto; }
    nav button { width: auto; flex-shrink: 0; }
    nav .setup-link { margin: 0; }
    .settings-content { padding: 20px; }
  }
</style>
