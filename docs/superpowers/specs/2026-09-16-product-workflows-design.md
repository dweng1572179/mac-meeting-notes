# Meeting Notes product workflows

The user authorized autonomous development, testing, public release, and safe installation, and now requests a fuller product with better UI, error continuity, and accent support. This release improves the existing native workflow without replacing its architecture.

## Product decisions

1. Add durable transcription settings: automatic or explicit language, a vocabulary of names and technical terms, and lower-cost or higher-accuracy OpenAI transcription. Preserve the existing lower-cost default. Capture the settings with each recording so retries are reproducible. Names/context may guide recognition but must never substitute invented speech. These are recognition hints, not voice training or guaranteed accent recognition.
2. Reveal the saved transcript, including partial progress, with literal search, safe highlighting, source/offset text, copy, and a Markdown export containing original notes, enhanced notes, transcript, and capture warnings. Add meeting-specific questions using the existing grounded answer/citation pipeline.
3. Make failure recovery specific and continuous: keep exact backend errors/request IDs, distinguish transcription from enrichment, route key/access failures to settings, explain silence without claiming a capture fault, offer a new meeting without deleting the old one, and let an explicit no-speech retry retry empty chunks while preserving nonblank checkpoints. Fix Settings losing structured errors. Retain typed questions on failure. Make saving and recording prerequisites visible.

## Interfaces

- `TranscriptionSettings { language: string, vocabulary: string, model: string }`; empty language means automatic; allowed models `gpt-4o-mini-transcribe` (default), `gpt-4o-transcribe`; vocabulary limit 2000 Unicode characters. Validate all fields natively. Settings are nonsecret local JSON with atomic writes and existing path protections.
- Bootstrap adds `settings`; Tauri `save_transcription_settings(settings)` returns validated settings. Session adds backward-compatible `transcriptionSettings`, captured at recording start. Existing sessions preserve default behavior.
- Existing `ask_meetings` accepts optional `sessionId`; when present it loads exactly that meeting, validates source material/status, and never widens scope. Frontend `askMeeting(id, question)` wraps it.
- Existing `retry_processing` keeps its signature. Explicit retry of a `no_speech` failure makes only empty saved chunks pending, adopts current settings for that retry, and preserves all nonblank transcript checkpoints and original notes. An all-zero-frame source cannot become useful on retry; UI explains and offers a fresh note.

## Validation and boundaries

TDD for backend request hints, settings persistence/validation, single-meeting scoping, empty-result retry, and frontend recovery/search/export decisions. Run all Rust/frontend checks sequentially, inspect actual rendered workflows, verify a production signed app/DMG, and test a synthetic request with language/vocabulary settings. Never process or expose the recovered lecture. Never install or quit the user's app during active recording. No ffmpeg, Electron, extra runtime dependency, cloud sync, or new database.

## Reference research

- OpenWhispr's MIT community code exposes language choice and bounded custom dictionary hints: https://github.com/OpenWhispr/openwhispr/blob/main/src/utils/dictionaryPromptCap.js
- Anarlog's MIT community app exposes spoken languages and dictionary settings: https://github.com/fastrepl/anarlog/tree/main/apps/desktop/src/settings
- OpenAI documents prompts for proper names, technical vocabulary, and prior chunk context: https://developers.openai.com/api/docs/guides/speech-to-text

Use these workflow ideas with original implementation. No code copied and no enterprise components used. Device reconnection, diarization, live captions, screen context, calendar integration, team sync, and signing/notarization remain separate roadmap work with explicit limits; this release must not imply they are finished.
