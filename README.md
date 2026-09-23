# Meeting Notes

A lightweight Mac and Windows meeting notepad that records your microphone and computer audio while you type. Transcripts arrive section by section during the meeting. After you stop, it turns the conversation and your notes into a record you can review and edit.

## What it does

- Captures the default microphone and computer audio together without inviting a meeting bot.
- Finalizes native AAC sections on Mac or PCM WAV sections on Windows around every 60 seconds and transcribes them sequentially while recording continues. Stop saves the final tails and finishes pending work; this is section-based transcription, not word-by-word live captions.
- Saves each completed transcription before deleting its audio. Ordinary retries reuse saved progress; dense sections that reach the model output limit are split further. Transient failures retry automatically while capture continues, with a capped delay after the first fast retries. Stop starts a fresh, finite final-processing attempt; permanent errors still need attention.
- Prevents automatic idle system sleep while recording. The display may turn off; keep the lid open and power connected. Manual sleep, lid closure and a depleted battery can still interrupt capture.
- Shows missing/stalled-source warnings and separate elapsed/captured durations. Partial transcripts and capture warnings survive interruptions.
- Opens every meeting as one editable **Notes** document, with **Transcript** as its reference. Notes autosave before navigation, export, and questions. Older meetings retain manual text missing from their summary; deleting a transcript keeps the notes.
- Displays timed speaker turns when speaker detection is selected, with searchable text, source labels, topic navigation, and access to the raw transcript. Plain transcripts use paragraphs and quiet timestamps instead of repeating section headings. Invalid speaker annotations fall back to the returned transcript text with a notice, without another paid request. Speaker labels are local to each source/upload; matching labels in different sections do not identify the same person.
- Produces notes plus suggested title, context, category, participants, and topic anchors in one structured enrichment response. Suggestions carry exact supporting excerpts and require your acceptance; they never silently replace manual fields. Suggestions without verified evidence are omitted while usable notes are kept.
- **Update notes from transcript** refreshes a completed meeting using its current notes and saved transcript. It keeps one immediately previous document; **Restore previous notes** swaps the two versions and can be reversed. Edits made while generation runs take priority over its result. This does not recreate missing audio or add speaker attribution to older plain transcripts.
- Exports Markdown to Downloads with notes, transcript, meeting details, and capture warnings. Incomplete processing is labeled.
- Saves language and recognition preferences. New installations with no saved preferences default to speaker detection; existing saved choices remain unchanged. Text-only options retain vocabulary hints.
- Organizes meetings with local categories/folders, suggestions from existing category names, a timeline-style library, and search across meeting details, notes, and transcripts.
- Answers meeting and library questions with saved-passage citations, readable formatted answers, copy, and expandable sources. Follow-ups include up to four prior turns to resolve references; current saved passages remain the evidence. Oversized source selections produce an explicit error.
- Keeps a separate conversation for each meeting, folder, and the whole library. Full answers, citations, and drafts survive navigation and restart; a request can finish after you leave its view. An interrupted request requires an explicit retry instead of automatically repeating a paid call. Only follow-up request context is shortened. Starting a new conversation preserves your draft, and storage failures are visible.

## Onboarding and settings

No Meeting Notes account is required. First use offers local typed notes or optional recording/AI setup; existing libraries open normally. Settings separates AI credentials, recording defaults/permissions, privacy/data, and connection status. A locked credential store leaves local notes available. Removing a key clears only this computer’s saved credential and is blocked during recording/processing; it does not revoke the key or cancel requests already sent.

Notes use compact expandable meeting details. Chat sits beside the document in wider windows and opens from a dock at smaller widths; its composer stays accessible while answers scroll.

Google/Microsoft calendar connections and mailbox import are not implemented. See the [onboarding and integration guide](docs/onboarding-and-integrations.md) for the calendar-first security recommendation and provider registration requirements.

## Privacy

Your API key stays in macOS Keychain or Windows Credential Manager. Meeting audio, transcripts, original notes, and the selected meeting sources used for a question are sent directly to OpenAI as needed; this app is not fully local or offline.

Enrichment uses the OpenAI Responses API with `store: false`. OpenAI states that API data is not used to train its models unless you opt in. Default abuse-monitoring logs may contain customer content and may be retained for up to 30 days. OpenAI documents no application-state retention for `/v1/audio/transcriptions`.

See OpenAI's [data controls](https://developers.openai.com/api/docs/guides/your-data) and [API reference overview](https://developers.openai.com/api/reference/overview) for the current policy and authentication guidance.

## Requirements

- Apple silicon Mac with macOS 14.2 or later, or an x64 Windows laptop/desktop (Windows 11 recommended; Windows 10 22H2 compatibility target).
- Recording and AI: OpenAI API key with paid API access; API usage is billed separately from ChatGPT. Typed notes need no key.
- Mac: Microphone and System Audio Recording permissions. Windows: microphone access for desktop apps and an available audio output device.
- One-time unnotarized-app approval: right-click **Open** on macOS 14; **Open Anyway** in Privacy & Security on macOS 15 or later

## Download

Choose the installer for your computer from the latest GitHub release:

- **Mac:** [`Meeting-Notes.dmg`](https://github.com/dweng1572179/mac-meeting-notes/releases/latest/download/Meeting-Notes.dmg)
- **Windows x64 beta:** [`Meeting-Notes-Setup.exe`](https://github.com/dweng1572179/mac-meeting-notes/releases/latest/download/Meeting-Notes-Setup.exe). See the [v0.5.0 release notes](docs/releases/v0.5.0.md) for validation and known limits.

Do not install development or interim builds. Release checksums are included as `SHA256SUMS.txt`.

## First launch on Mac

1. Open `Meeting-Notes.dmg` and drag **Meeting Notes** to **Applications**.
2. **Stop every recording and quit Meeting Notes before replacing or updating it.** If Meeting Notes already exists in Applications, choose **Replace**. Never choose **Keep Both**, which creates duplicate app copies.
3. Approve the unnotarized beta once. On **macOS 14**, right-click **Meeting Notes**, choose **Open**, then confirm **Open**. On **macOS 15 or later**, first try opening Meeting Notes from Applications and dismiss the warning, then open **System Settings → Privacy & Security → Open Anyway** and confirm opening Meeting Notes.
4. Choose **Start with typed notes** to work locally, or **Set up recording & AI** to add your key. On your first recording, grant both Microphone and System Audio Recording when macOS asks.

Future launches work normally from Applications. Updates to this ad-hoc signed beta change its privacy identity and may require granting audio permissions again. The app detects executable replacement while running and warns about possible capture loss; only stable Developer ID signing/notarization can address the underlying identity problem.

## First launch on Windows

1. Download **Meeting-Notes-Setup.exe** from the latest release. Stop recording and quit Meeting Notes before any update.
2. Run the installer. It installs for your Windows account and installs/updates Microsoft WebView2 if necessary. Internet access is needed for a missing runtime.
3. This beta does not have an Authenticode signing certificate. If SmartScreen blocks it, verify it came from this repository and check its published SHA-256 before choosing **More info → Run anyway**, if that option is available. Managed laptops may require an administrator's approval.
4. Open Meeting Notes and choose typed notes or optional AI setup. An AI key is saved in Windows Credential Manager. Manage it in **Settings → AI & API key**.
5. In **Settings → Privacy & security → Microphone**, enable microphone access and **Let desktop apps access your microphone**. Select the microphone and default output device in Windows Sound settings before recording.
6. For an in-person class, use the laptop microphone. For an online meeting in Edge, Chrome, Teams, or Zoom, play audio through the selected default Windows output device. Recording starts only when you press Record.

The Windows recorder captures the default microphone and default output endpoint. Devices remain selected for that recording; stop and start again after changing them. Audio sent to a different output endpoint will not be captured. Headphones help avoid recording computer playback twice through speakers and microphone.

Windows uses native WASAPI and mono 16 kHz PCM WAV sections, without an FFmpeg dependency. Each track uses about 1.9 MB/minute while retained: a fully offline 110-minute two-track session can retain about 422 MB. Successfully checkpointed speech sections can be removed as usual. API transcription cost depends on duration, not WAV file size. Native Windows CI passed compilation, synthetic recording/recovery, credential persistence, installer creation, and install/reinstall/launch/uninstall with synthetic-data preservation. See the [validation receipt](docs/validation/v0.5.0-receipt.md) and [v0.5.0 release notes](docs/releases/v0.5.0.md). Physical Windows microphone/loopback, Bluetooth, device switching, sleep/wake, and real Edge audio remain unverified, as does WebView2 installation on a clean PC without an existing runtime.

## Language, accents, and terminology

Open **Settings → Recording** to choose automatic detection or a spoken language and one of these recognition modes:

- **Speaker detection** (`gpt-4o-transcribe-diarize`): timed speaker turns, with identities scoped to each source and uploaded section. It does not support vocabulary prompts; saved vocabulary is retained for text-only modes.
- **Text only · lowest cost** (`gpt-4o-mini-transcribe`) or **Text only · higher accuracy** (`gpt-4o-transcribe`): plain transcription with optional names and terminology, up to 2,000 characters.

New/unset preferences choose speaker detection. Explicitly saved preferences and historical recordings keep their earlier meaning. Recognition settings do not train a personal voice model or guarantee accent accuracy. Use automatic detection for mixed-language meetings.

OpenAI bills transcription for each uploaded audio track. Enrichment uses one `gpt-4.1-mini-2025-04-14` structured response after transcription finishes, or when you explicitly refresh a completed meeting. The app does not repeatedly summarize the whole meeting during recording. Recognition mode, audio duration, question usage, refreshes, and retries affect your API bill.

Settings apply when a new recording starts. Normal retries keep that recording's settings and skip saved chunks. An explicit retry after **No speech detected** uses your current settings and retranscribes only empty results; this can incur another API charge. A source with zero captured frames cannot be recovered by transcription. **New meeting** starts a separate note without deleting the old one.

Connect AirPods or other Bluetooth devices and select the intended system input before starting. Mid-recording device reconnection/rebinding is not implemented. Capture warnings report missing/stalled frames, not speech intelligibility.

## Build from source

Install Node.js 22.12 or later, the stable Rust toolchain, and Xcode Command Line Tools, then run:

```sh
npm ci
rustup target add aarch64-apple-darwin
npm test -- --run
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --target aarch64-apple-darwin --bundles app
node scripts/package-dmg.mjs "src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Meeting Notes.app" Meeting-Notes.dmg
```

The verified `Meeting-Notes.dmg` is written in the repository root. Packaging uses native `hdiutil` with APFS and checks the app signature inside the mounted image; it avoids metadata changes introduced by the default DMG packaging path on newer macOS versions.

On Windows, install Node.js 22.12+, stable Rust (MSVC), Microsoft C++ Build Tools with the Desktop development with C++ workload, and WebView2. Run:

```powershell
npm ci
npm test -- --run
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --target x86_64-pc-windows-msvc --bundles nsis
```

The installer is under `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`.

## Focused evaluation

The repeatable labeled simulation and expected results are in [`docs/evaluation.md`](docs/evaluation.md). The [September 22 reliability audit](docs/reliability-audit-2026-09-22.md) documents repeated failure causes, v0.4.4 fixes, verified cost estimates and remaining limits.

## Storage

Notes and chats are not encrypted by the app. **Settings → Privacy & data** explains storage and opens the native data folder. Copying that folder alone does not back up separate WebView conversation storage.

Meeting metadata, your notes, raw transcripts, generated AI notes, your AI-note edits, suggestions, and transcription progress are stored locally in the app's platform-specific application-data directory. The interface presents one editable Notes document. One previous document is retained for reversible restoration after enhancement; this is not unlimited version history.

New recordings use numbered source sections. A finalized section becomes eligible for transcription only after capture closes and syncs its file. Transcription is atomically saved before successfully checkpointed speech audio can be deleted; silent, failed, or unreadable audio may remain for recovery. Startup/retry can recover unregistered section files and retains unreadable tails with warnings. Legacy whole-file recordings keep their retry path.

Question conversations are saved separately in the app’s local webview storage. Deleting a meeting or transcript first clears related saved AI answers; unrelated conversations and aggregate drafts stay. Cleanup failure blocks deletion, and the confirmation explains the affected history.

A failure between an API response and its durable checkpoint can require that request again. Deleting a meeting also deletes its contained retained audio. The API key is stored separately in macOS Keychain or Windows Credential Manager.

## Limitations

- Transcription and enrichment require internet access and paid OpenAI API access.
- Library questions use at most the 20 most recent completed meetings in the selected scope. **Ask this meeting** uses exactly the open completed meeting. Combined source text is limited to 200,000 UTF-8 bytes; oversized selections are rejected explicitly.
- Language/vocabulary controls are not an accent benchmark. Physical Bluetooth switching and long real-world capture still need broader validation.
- Folders and notes are local to one computer; there are no accounts, shared workspaces, or team sync.
- This beta is ad-hoc signed but not notarized. macOS 14 requires a one-time right-click **Open**; macOS 15 or later requires trying the first launch, then **System Settings → Privacy & Security → Open Anyway**.
- Model output can be incomplete or wrong. Review notes, suggestions, and speaker turns against the source transcript. A supporting quote does not make an inference infallible.
- Speaker labels are local to each source/upload, not a verified participant directory. Cross-section identity matching and Zoom/Teams name association are not implemented.
- Completed legacy transcripts can receive refreshed notes and suggestions, but plain text alone cannot recover voice identities.

## Reliability verification

`cargo test --manifest-path src-tauri/Cargo.toml` includes native synthetic AAC rotation, concurrent callback/stop boundaries, exact frame totals, safe sink retirement, failed preparation recovery, partial-finalization detection, and the existing more-than-ten-minute decoding/retry checks. Local fixtures also cover durable transcript checkpoints and request/response contracts. No microphone speech is needed for these checks.

Opt-in paid tests use macOS `say`, the app’s existing login Keychain entry, and generated simulation content in a disposable library. The legacy long-recording check and the new segmented speaker-detection check run separately:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib live_synthetic_long_recording_with_openai -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib live_synthetic_segmented_diarization_with_openai -- --ignored --nocapture --test-threads=1
```

Chunk offsets describe recorded audio, not guaranteed wall-clock timing through capture gaps. The app cannot reconstruct audio that macOS did not deliver. Silence is different from missing frames; health counters do not prove speech was audible. Persistent no-frame warnings can also mean there is no system audio playing.

## Roadmap

- Broader ask-all-meetings retrieval beyond the current explicit source budget.
- Automatic microphone/device reconnection and live input-level feedback.
- Word-by-word live captions and importing existing audio.
- Cross-section speaker identity matching and optional Zoom/Teams participant-name association.
- Optional screen-share or visual context.
- Workspace organization and polish.
- Stable Developer ID signing and notarization when a signing identity is available.

## Community references

The language/vocabulary and meeting-workspace workflows were informed by the MIT community projects [OpenWhispr](https://github.com/OpenWhispr/openwhispr) and [Anarlog](https://github.com/fastrepl/anarlog). This release uses an original implementation with the app's existing native dependencies; no code or enterprise components were copied.

## License

[MIT](LICENSE) © 2026 Darryl Weng
