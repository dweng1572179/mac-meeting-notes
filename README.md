# Meeting Notes

A lightweight macOS meeting notepad that records your microphone and computer audio while you type. Transcripts arrive section by section during the meeting. After you stop, it turns the conversation and your notes into a record you can review and edit.

## What it does

- Captures the default microphone and Mac computer audio together without inviting a meeting bot.
- Finalizes native AAC sections around every 60 seconds and transcribes them sequentially while recording continues. Stop saves the final tails and finishes pending work; this is section-based transcription, not word-by-word live captions.
- Saves each completed transcription before deleting its audio. Ordinary retries reuse saved progress; dense sections that reach the model output limit are split further. A network failure pauses transcription without stopping capture.
- Shows missing/stalled-source warnings and separate elapsed/captured durations. Partial transcripts and capture warnings survive interruptions.
- Opens every meeting as one editable **Notes** document, with **Transcript** as its reference. Notes autosave before navigation, export, and questions. Older meetings retain manual text missing from their summary; deleting a transcript keeps the notes.
- Displays timed speaker turns when speaker detection is selected, with searchable text, source labels, topic navigation, and access to the raw transcript. Plain transcripts use paragraphs and quiet timestamps instead of repeating section headings. Speaker labels are local to each source/upload; matching labels in different sections do not identify the same person.
- Produces notes plus suggested title, context, category, participants, and topic anchors in one structured enrichment response. Suggestions carry exact supporting excerpts and require your acceptance; they never silently replace manual fields. Suggestions without verified evidence are omitted while usable notes are kept.
- **Update notes from transcript** refreshes a completed meeting using its current notes and saved transcript. Edits made while that request runs take priority over its result. This does not recreate missing audio or add speaker attribution to older plain transcripts.
- Exports Markdown to Downloads with notes, transcript, meeting details, and capture warnings. Incomplete processing is labeled.
- Saves language and recognition preferences. New installations with no saved preferences default to speaker detection; existing saved choices remain unchanged. Text-only options retain vocabulary hints.
- Organizes meetings with local categories/folders, a timeline-style library, and search across meeting details, notes, and transcripts.
- Answers meeting and library questions with saved-passage citations. The model selects passage IDs and the app supplies the exact source text; unavailable answers are normal responses. Oversized source selections produce a clear error instead of silently dropping later decisions. Both question views use a compact, expanding composer with Command/Ctrl+Enter and expandable sources. Failed requests keep the question for retry.

## Privacy

Your API key stays in macOS Keychain. Meeting audio, transcripts, original notes, and the selected meeting sources used for a question are sent directly to OpenAI as needed; this app is not fully local or offline.

Enrichment uses the OpenAI Responses API with `store: false`. OpenAI states that API data is not used to train its models unless you opt in. Default abuse-monitoring logs may contain customer content and may be retained for up to 30 days. OpenAI documents no application-state retention for `/v1/audio/transcriptions`.

See OpenAI's [data controls](https://developers.openai.com/api/docs/guides/your-data) and [API reference overview](https://developers.openai.com/api/reference/overview) for the current policy and authentication guidance.

## Requirements

- Apple silicon Mac
- macOS 14.2 or later
- OpenAI API key with paid API access; API usage is billed by OpenAI
- Microphone and System Audio Recording permissions
- One-time unnotarized-app approval: right-click **Open** on macOS 14; **Open Anyway** in Privacy & Security on macOS 15 or later

## Download

Download the final [`Meeting-Notes.dmg`](https://github.com/dweng1572179/mac-meeting-notes/releases/latest/download/Meeting-Notes.dmg) from the latest GitHub release. Do not install development or interim builds.

## First launch

1. Open `Meeting-Notes.dmg` and drag **Meeting Notes** to **Applications**.
2. **Stop every recording and quit Meeting Notes before replacing or updating it.** If Meeting Notes already exists in Applications, choose **Replace**. Never choose **Keep Both**, which creates duplicate app copies.
3. Approve the unnotarized beta once. On **macOS 14**, right-click **Meeting Notes**, choose **Open**, then confirm **Open**. On **macOS 15 or later**, first try opening Meeting Notes from Applications and dismiss the warning, then open **System Settings → Privacy & Security → Open Anyway** and confirm opening Meeting Notes.
4. Add your OpenAI API key in the app's settings. On your first recording, grant both Microphone and System Audio Recording when macOS asks.

Future launches work normally from Applications. Updates to this ad-hoc signed beta change its privacy identity and may require granting audio permissions again. The app detects executable replacement while running and warns about possible capture loss; only stable Developer ID signing/notarization can address the underlying identity problem.

## Language, accents, and terminology

Open **Settings → Transcription** to choose automatic detection or a spoken language and one of these recognition modes:

- **Speaker detection** (`gpt-4o-transcribe-diarize`): timed speaker turns, with identities scoped to each source and uploaded section. It does not support vocabulary prompts; saved vocabulary is retained for text-only modes.
- **Text only · lowest cost** (`gpt-4o-mini-transcribe`) or **Text only · higher accuracy** (`gpt-4o-transcribe`): plain transcription with optional names and terminology, up to 2,000 characters.

New/unset preferences choose speaker detection. Explicitly saved preferences and historical recordings keep their earlier meaning. Recognition settings do not train a personal voice model or guarantee accent accuracy. Use automatic detection for mixed-language meetings.

OpenAI bills transcription for each uploaded audio track. Enrichment uses one `gpt-4.1-mini-2025-04-14` structured response after transcription finishes, or when you explicitly refresh a completed meeting. The app does not repeatedly summarize the whole meeting during recording. Recognition mode, audio duration, question usage, refreshes, and retries affect your API bill.

Settings apply when a new recording starts. Normal retries keep that recording's settings and skip saved chunks. An explicit retry after **No speech detected** uses your current settings and retranscribes only empty results; this can incur another API charge. A source with zero captured frames cannot be recovered by transcription. **New meeting** starts a separate note without deleting the old one.

Connect AirPods or other Bluetooth devices and select the intended macOS input before starting. Mid-recording device reconnection/rebinding is not implemented. Capture warnings report missing/stalled frames, not speech intelligibility.

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

## Focused evaluation

The repeatable labeled simulation and expected results are in [`docs/evaluation.md`](docs/evaluation.md).

## Storage

Meeting metadata, your notes, raw transcripts, generated AI notes, your AI-note edits, suggestions, and transcription progress are stored locally in the app's macOS application-data directory. The generated AI-note baseline and your edited version are separate.

New recordings use numbered source sections. A finalized section becomes eligible for transcription only after capture closes and syncs its file. Transcription is atomically saved before successfully checkpointed speech audio can be deleted; silent, failed, or unreadable audio may remain for recovery. Startup/retry can recover unregistered section files and retains unreadable tails with warnings. Legacy whole-file recordings keep their retry path.

A failure between an API response and its durable checkpoint can require that request again. Deleting a meeting also deletes its contained retained audio. The API key is stored separately in macOS Keychain.

## Limitations

- Transcription and enrichment require internet access and paid OpenAI API access.
- Library questions use at most the 20 most recent completed meetings in the selected scope. **Ask this meeting** uses exactly the open completed meeting. Combined source text is limited to 200,000 UTF-8 bytes; oversized selections are rejected explicitly.
- Language/vocabulary controls are not an accent benchmark. Physical Bluetooth switching and long real-world capture still need broader validation.
- Folders and notes are local to one Mac; there are no accounts, shared workspaces, or team sync.
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
