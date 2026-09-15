# Meeting Notes

A lightweight macOS meeting notepad that records your microphone and computer audio while you type. When the meeting ends, it uses your sparse notes as the emphasis signal and turns both sides of the conversation into a useful record.

## What it does

- Captures the default microphone and Mac computer audio together without inviting a meeting bot.
- Sends each audio source to OpenAI for transcription, labels `You` and `Meeting audio`, then enriches the transcript around your original notes.
- Keeps Original and Enhanced views in a local meeting library that survives app restarts.
- Organizes meetings with simple local folders and a timeline-style All meetings view.
- Answers questions across a folder or the latest completed meetings, with clickable exact-source citations.
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
2. If Meeting Notes already exists in Applications, choose **Replace**. Never choose **Keep Both**, which creates duplicate app copies.
3. Approve the unnotarized beta once. On **macOS 14**, right-click **Meeting Notes**, choose **Open**, then confirm **Open**. On **macOS 15 or later**, first try opening Meeting Notes from Applications and dismiss the warning, then open **System Settings → Privacy & Security → Open Anyway** and confirm opening Meeting Notes.
4. Add your OpenAI API key in the app's settings. On your first recording, grant both Microphone and System Audio Recording when macOS asks.

Future launches work normally from Applications.

## Build from source

Install Node.js 22.12 or later, the stable Rust toolchain, and Xcode Command Line Tools, then run:

```sh
npm ci
rustup target add aarch64-apple-darwin
npm test -- --run
npm run check
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --target aarch64-apple-darwin
```

The DMG is written under `src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/`.

## Focused evaluation

The repeatable labeled simulation and expected results are in [`docs/evaluation.md`](docs/evaluation.md).

## Storage

Meeting metadata, original notes, transcripts, and enhanced notes are stored locally in the app's macOS application-data directory. Raw audio is retained only when needed for recording, transcription, or retry, and is deleted after successful transcription. Deleting a meeting also deletes its contained retained audio. The API key is stored separately in macOS Keychain.

## Limitations

- Transcription and enrichment require internet access and paid OpenAI API access.
- Meeting questions intentionally use at most the 20 most recent completed meetings in the current scope.
- Folders and notes are local to one Mac; there are no accounts, shared workspaces, or team sync.
- This beta is ad-hoc signed but not notarized. macOS 14 requires a one-time right-click **Open**; macOS 15 or later requires trying the first launch, then **System Settings → Privacy & Security → Open Anyway**.
- Model output can be incomplete or wrong; review enhanced notes against the Original view.

## License

[MIT](LICENSE) © 2026 Darryl Weng
