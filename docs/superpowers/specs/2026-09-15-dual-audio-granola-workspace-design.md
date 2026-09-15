# Meeting Notes v0.2: Dual Audio and Local Meeting Intelligence

## Goal

Turn the existing Meeting Notes beta into a dependable Granola-like macOS meeting workspace while preserving its defining constraints: one downloadable app, local-first storage, no bot, no hosted application backend, and a user-provided OpenAI API key.

The release must capture both sides of a normal call, keep the user's sparse notes as the priority signal, make completed meetings easy to browse by folder, and answer questions across a selected set of meetings with traceable sources.

## Approved Scope

### Capture and processing

- Record the default macOS output and default microphone at the same time.
- Keep the two sources as separate temporary AAC files instead of adding a real-time mixer.
- Transcribe both files, label their text as `Meeting audio` and `You`, and pass the combined transcript with the user's original notes into enrichment.
- Treat two blank transcriptions as a failed capture. Never generate a metadata-only “successful” summary.
- Preserve both audio files when capture, transcription, or persistence fails. Delete them only after a non-empty transcript has been saved.
- Continue to store the OpenAI key only in macOS Keychain.

### Granola-style local workspace

- Keep the calm, document-first meeting editor and compact persistent navigation.
- Add a lightweight folder field to meetings. Folder names are derived from saved meetings, so no folder database or separate folder lifecycle is needed.
- Turn Home into an `All meetings` timeline and allow selecting a folder from the sidebar.
- Add an `Ask your meetings` panel beside the timeline. It answers against All meetings or the selected folder.
- Require every AI answer to return source records. Each source identifies a meeting and includes an exact supporting excerpt; selecting a source opens that meeting.
- Keep chat ephemeral in v0.2. The durable source of truth remains the meeting notes and transcripts.

This mirrors the useful Granola 2.0 structure—folder timelines, cross-meeting questions, and source-linked answers—without copying its branding or adding its team service layer.

## Explicit Non-Goals

- Accounts, teams, shared URLs, permissions, Slack posting, public browsing, or cloud sync.
- Drag-and-drop folder management, empty folders, nested folders, or folder templates.
- Multiple AI providers or a model picker; the product remains BYO OpenAI key.
- Embeddings, a vector database, a transcript upload flow, or a bundled AI model.
- Pixel-copying Granola artwork, logos, names, or proprietary assets.
- A custom real-time audio mixer. Separate transcription retains source identity with far less native audio risk.

## Architecture

### Native recording

Reuse the existing Core Audio process tap for system output. Add a second Core Audio IO callback on the current default input device and write it through the same `ExtAudioFile` AAC path already used by the system recorder. The native recording owner starts and tears down both streams as one transaction.

The bundle adds `NSMicrophoneUsageDescription`. Starting a meeting requires both capture paths; if either cannot start, both are stopped and the UI explains the relevant macOS privacy setting. This avoids silently producing incomplete meetings.

### Session data

Extend the existing JSON session document with backward-compatible optional fields:

```json
{
  "folder": "Acquisitions",
  "microphoneAudioPath": "/local/app-data/audio/<id>-mic.m4a"
}
```

Older sessions deserialize with an empty folder and no microphone path. Path validation accepts only the exact application-owned system and microphone filenames for the matching session.

### Processing lifecycle

1. Stop and finalize both capture writers.
2. Persist both retained paths before starting network work.
3. Transcribe any source that does not yet have durable transcript text.
4. Reject the result when both source transcripts are blank.
5. Combine non-empty transcripts with explicit source labels and persist the combined text.
6. Delete both raw audio files only after the transcript is durably saved.
7. Enrich from the latest saved session so notes typed during processing are included.
8. On any failure, retain remaining audio and expose Retry.

For backward compatibility, retry continues to process legacy system-only recordings.

### Folder intelligence

The frontend derives the folder list and filtered meeting timeline from the sessions already returned by bootstrap. One new Rust command receives a folder filter and question, loads the eligible completed sessions, and sends numbered meeting sources to the OpenAI Responses API with `store: false`.

The response uses a strict JSON schema containing an answer and citations. The native layer discards citations whose meeting ID is not in the supplied set or whose excerpt is not present in that meeting's notes/transcript. The UI renders the accepted citations as buttons that open their source meetings.

To keep the first local implementation predictable, the query uses the most recent 20 eligible meetings and bounded text per meeting. Local retrieval can replace that limit only when real libraries make it necessary.

## Interface

- Sidebar: Meeting Notes wordmark, search, All meetings, New note, derived Folders, Recent, Settings.
- Library view: folder title and meeting timeline on the left; source-grounded `Ask your meetings` panel on the right.
- Meeting view: existing spacious editor, plus a quiet folder field and clearer source state.
- Recording control: explicitly says `Microphone + computer` while live.
- Processing and failure states stay inline and preserve the current visual language.
- The design uses warm paper surfaces, compact neutral controls, restrained color, clear hover/pressed/focus states, and generous writing space. It follows the hierarchy visible in Granola 2.0 while remaining an independently branded product.

## Error Handling and Privacy

- Microphone and system-audio permission failures name the exact System Settings location.
- A blank dual transcription produces `No speech was detected. Your audio was kept so you can retry.`
- Folder questions fail visibly when no completed meeting contains source material.
- Folder chat sends only the selected meetings' notes/transcripts directly to OpenAI. Nothing is uploaded to an application server.
- Deleting a meeting removes both application-owned audio files if retained.

## Verification

- Unit checks for backward-compatible session loading, exact dual-audio path validation, transcript combination, blank-capture rejection, and citation validation.
- Existing persistence, lifecycle, API-contract, TypeScript, and Svelte checks remain green.
- Native acceptance test records a synthetic phrase from system output and a different phrase through the microphone, stops normally, and verifies both appear in the combined transcript and enriched notes.
- Product acceptance test creates meetings in two folders, asks a folder question, follows a citation to its meeting, quits, reopens, and verifies notes/folders remain available.

## References

- [Granola 2.0: A second brain for your team](https://www.granola.ai/blog/two-dot-zero)
- [Apple: Capturing system audio with Core Audio taps](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps)
- [OpenAI: Audio transcription](https://developers.openai.com/api/docs/guides/speech-to-text)
- [OpenAI: Responses API](https://developers.openai.com/api/reference/responses/overview)
