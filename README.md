# Meeting Notes

A macOS meeting notepad that captures system audio while you type, then combines the transcript with your original notes into a clearer summary, key points, decisions, and action items. Your original notes remain available verbatim beside the enhanced version.

## What it does

- Captures Mac system audio and accepts sparse typed notes during a meeting.
- Sends recorded audio to OpenAI for transcription, then sends the transcript and meeting notes to OpenAI for enrichment.
- Keeps Original and Enhanced views in a local meeting library that survives app restarts.
- Deletes raw audio after successful transcription. Failed transcription can retain audio locally so you can retry.

## Privacy

Your API key stays in macOS Keychain. Meeting audio, transcripts, and original notes are sent directly to OpenAI as needed; this app is not fully local or offline.

Enrichment uses the OpenAI Responses API with `store: false`. OpenAI states that API data is not used to train its models unless you opt in. Default abuse-monitoring logs may contain customer content and may be retained for up to 30 days. OpenAI documents no application-state retention for `/v1/audio/transcriptions`.

See OpenAI's [data controls](https://developers.openai.com/api/docs/guides/your-data) and [API reference overview](https://developers.openai.com/api/reference/overview) for the current policy and authentication guidance.

## Requirements

- Apple silicon Mac
- macOS 14.2 or later
- OpenAI API key with paid API access; API usage is billed by OpenAI
- System Audio Recording permission
- One-time right-click **Open** because the beta is unsigned

## Download

Download the final [`Meeting-Notes.dmg`](https://github.com/dweng1572179/mac-meeting-notes/releases/latest/download/Meeting-Notes.dmg) from the latest GitHub release. Do not install development or interim builds.

## First launch

1. Open `Meeting-Notes.dmg` and drag **Meeting Notes** to **Applications**.
2. If Meeting Notes already exists in Applications, choose **Replace**. Never choose **Keep Both**, which creates duplicate app copies.
3. In Applications, right-click **Meeting Notes** and choose **Open** once, then confirm **Open**.
4. Add your OpenAI API key in the app's settings and grant System Audio Recording permission when prompted.

Future launches work normally from Applications.

## Build from source

Install Node.js 20, the stable Rust toolchain, and Xcode Command Line Tools, then run:

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

The repeatable labeled simulation, expected results, and current live-test status are in [`docs/evaluation.md`](docs/evaluation.md). The live OpenAI/system-audio acceptance item remains unexecuted without a paid API key; automated contract, UI, restart, and storage checks are documented there.

## Storage

Meeting metadata, original notes, transcripts, and enhanced notes are stored locally in the app's macOS application-data directory. Raw audio is retained only when needed for recording, transcription, or retry, and is deleted after successful transcription. Deleting a meeting also deletes its contained retained audio. The API key is stored separately in macOS Keychain.

## Limitations

- Version 1 captures system audio only, not microphone input.
- Transcription and enrichment require internet access and paid OpenAI API access.
- This beta is unsigned and not notarized, so the one-time right-click Open step is required.
- Model output can be incomplete or wrong; review enhanced notes against the Original view.

## License

[MIT](LICENSE) © 2026 Darryl Weng
