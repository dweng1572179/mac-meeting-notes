# Meeting Notes

A lightweight macOS meeting notepad that records your microphone and computer audio while you type. When the meeting ends, it uses your sparse notes as the emphasis signal and turns both sides of the conversation into a useful record.

## What it does

- Captures the default microphone and Mac computer audio together without inviting a meeting bot.
- Splits each source into approximately five-minute native AAC chunks, transcribes them sequentially, and enriches the conversation around your original notes. Source labels and recording offsets preserve overlapping tracks without inventing a speaker identity.
- Saves every completed chunk before deleting its temporary audio. Retry resumes saved progress; dense chunks that reach the model output limit are split further.
- Shows live missing/stalled-source warnings and separate elapsed/captured durations; retains partial-capture warnings with the meeting.
- Keeps Original, Enhanced, and searchable Transcript views in a local meeting library that survives app restarts. Partial transcripts stay visible when processing fails.
- Exports a complete Markdown copy to Downloads, including original notes, transcript, context, and capture warnings; incomplete processing is clearly labeled.
- Saves transcription language, names/vocabulary, and an optional higher-accuracy model. Standard lower-cost transcription remains the default.
- Organizes meetings with simple local folders and a timeline-style All meetings view.
- Answers questions about one meeting, a folder, or the latest completed meetings, with exact-source citations. Questions include the complete saved source text within an explicit size budget; oversized selections produce a clear error instead of silently dropping later decisions.
- Searches titles, attendees, folders, original notes, enhanced notes, and transcripts; accented names can be found without typing accent marks.
- Offers specific recovery actions for missing keys, interrupted transcription, enrichment failures, and no-speech recordings. Saved progress and typed notes stay intact.
- Deletes raw audio after successful transcription. Failed transcription can retain audio locally so you can retry.

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

Open **Settings → Transcription** to choose automatic detection or a spoken language, add names and terminology (up to 2,000 characters), and select Standard (`gpt-4o-mini-transcribe`) or Higher accuracy (`gpt-4o-transcribe`). The latter costs more through your own OpenAI account. These controls can guide recognition of accented speech and uncommon words; they do not train a personal voice model or guarantee accuracy. Use automatic detection for mixed-language meetings.

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

Meeting metadata, original notes, transcripts, and enhanced notes are stored locally in the app's macOS application-data directory. Raw source audio is retained until its chunks have been safely checkpointed in the session JSON. Temporary chunk audio is deleted only after that chunk’s transcript is persisted. Failed or interrupted work keeps pending source audio; retry skips completed chunks. A failure between the API response and the durable checkpoint can require that uncheckpointed request again. Deleting a meeting also deletes its contained retained audio. The API key is stored separately in macOS Keychain.

## Limitations

- Transcription and enrichment require internet access and paid OpenAI API access.
- Library questions use at most the 20 most recent completed meetings in the selected scope. **Ask this meeting** uses exactly the open completed meeting. Combined source text is limited to 200,000 UTF-8 bytes; oversized selections are rejected explicitly.
- Language/vocabulary controls are not an accent benchmark. Physical Bluetooth switching and long real-world capture still need broader validation.
- Folders and notes are local to one Mac; there are no accounts, shared workspaces, or team sync.
- This beta is ad-hoc signed but not notarized. macOS 14 requires a one-time right-click **Open**; macOS 15 or later requires trying the first launch, then **System Settings → Privacy & Security → Open Anyway**.
- Model output can be incomplete or wrong; review enhanced notes against the Original view.

## Reliability verification

`cargo test --manifest-path src-tauri/Cargo.toml` covers native decoding of more than ten minutes of synthetic audio, independently valid chunks below the upload limit, middle-chunk API failure/reopen/retry, storage failures, empty and silent sources, source health, and exact safe OpenAI errors with request IDs. No microphone speech is needed.

An opt-in live test uses macOS `say`, the app’s existing login Keychain entry, and paid OpenAI access. It uses only generated simulation content and cleans its temporary library:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib live_synthetic_long_recording_with_openai -- --ignored --nocapture --test-threads=1
```

Chunk offsets describe recorded audio, not guaranteed wall-clock timing through capture gaps. The app cannot reconstruct audio that macOS did not deliver. Silence is different from missing frames; health counters do not prove speech was audible. Persistent no-frame warnings can also mean there is no system audio playing.

## Roadmap

- Broader ask-all-meetings retrieval beyond the current explicit source budget.
- Automatic microphone/device reconnection and live input-level feedback.
- Optional live captions and importing existing audio.
- Speaker diarization and optional Zoom/Teams participant-name association.
- Optional screen-share or visual context.
- Workspace organization and polish.
- Stable Developer ID signing and notarization when a signing identity is available.

## Community references

The language/vocabulary and meeting-workspace workflows were informed by the MIT community projects [OpenWhispr](https://github.com/OpenWhispr/openwhispr) and [Anarlog](https://github.com/fastrepl/anarlog). This release uses an original implementation with the app's existing native dependencies; no code or enterprise components were copied.

## License

[MIT](LICENSE) © 2026 Darryl Weng
