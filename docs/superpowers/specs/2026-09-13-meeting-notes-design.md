# Meeting Notes: Local macOS Meeting Notepad

## Goal

Build and publish a small, open-source macOS app that records computer audio while the user writes sparse notes, then uses the user's OpenAI API key to transcribe the meeting and enrich those notes. A user should download one DMG, open the app, enter an API key, and start a meeting without installing a meeting bot or running a server.

The app is named **Meeting Notes** and the public repository is named `mac-meeting-notes`. It is an independent implementation: the interaction and visual structure closely follow Granola's publicly documented desktop experience, but the app does not use Granola's name, logo, artwork, source code, or other proprietary assets.

## Success Criteria

- Capture macOS system audio during a live note-taking session.
- Keep typed notes, meeting context, captured audio, transcript, and enriched notes attached to one session.
- Preserve the user's emphasis instead of replacing it with a generic transcript summary.
- Distinguish tentative suggestions from later final decisions.
- Save completed and failed sessions locally and restore them after the app restarts.
- Never lose typed notes because audio capture or an API request failed.
- Ship a public MIT-licensed repository and one downloadable macOS DMG.
- Avoid Electron, SwiftUI, ScreenCaptureKit, a bundled browser, a database, and a hosted backend.

## Explicit Non-Goals

- Calendar sync, accounts, teams, sharing, mobile clients, meeting bots, templates, chat, and automatic updates.
- Microphone capture or mixing multiple audio sources in v1.
- Offline transcription or bundled AI models.
- Importing prerecorded audio or transcripts as the primary workflow.
- Copying Granola branding or proprietary assets.
- Apple Developer signing and notarization for the first release.

## Architecture

### Desktop shell and UI

Use Tauri 2 with a Svelte 5 and TypeScript frontend. Tauri renders the interface with macOS's existing WKWebView, so the distributed app does not include Chromium. Styling uses one local CSS system with platform and system fonts; no Tailwind or component library is needed.

The frontend owns presentation and temporary editor state. It calls a narrow set of Tauri commands for sessions, recording, settings, transcription, and enrichment. It never receives or stores the OpenAI API key.

### Native core

The Rust core owns privileged and durable work:

- Capture outgoing system audio with the macOS Core Audio process-tap APIs available on macOS 14.2 and later.
- Write a temporary audio file while recording.
- Persist session JSON atomically under `~/Library/Application Support/Meeting Notes/sessions/`.
- Store and retrieve the OpenAI API key with macOS Keychain.
- Send audio to OpenAI's speech-to-text API and send transcript, context, and original notes to OpenAI's text-generation API.
- Delete temporary audio after a successful transcription and retain it after failure for retry.

Rust bindings call Core Audio directly. The project builds with Apple Command Line Tools and the Rust toolchain; full Xcode is not required.

### Storage

Each session is one JSON document with this shape:

```json
{
  "id": "uuid",
  "title": "Investment committee call",
  "startedAt": "ISO-8601 timestamp",
  "endedAt": "ISO-8601 timestamp or null",
  "context": "Optional user-entered context",
  "attendees": ["Optional attendee names"],
  "originalNotes": "Sparse notes exactly as typed",
  "transcript": "Transcript or null",
  "enrichedNotes": "Generated Markdown or null",
  "status": "recording | processing | complete | failed",
  "error": "Recoverable error message or null",
  "audioPath": "Temporary local path or null"
}
```

Writes use a temporary file followed by an atomic rename. Notes autosave after a short debounce and immediately when recording stops or the window closes. No database or cache layer is introduced.

## Meeting Lifecycle

1. On first launch, the user enters an OpenAI API key. The Rust core validates it with a small request and stores it in Keychain.
2. The user creates a meeting with an optional title, context, and attendees.
3. Starting the meeting requests macOS system-audio permission and begins a Core Audio process tap.
4. The user writes sparse notes in the same document while a compact timer shows the recording state.
5. Stopping first closes the audio writer and persists the session, then starts transcription.
6. After transcription, the app sends the transcript, context, and original notes for enrichment.
7. On success, it saves the transcript and enriched notes, marks the session complete, and deletes the temporary audio.
8. On failure, it saves the error and audio path, leaves the original notes untouched, and offers Retry.

The app does not expose a transcript-upload shortcut. Every normal session begins in the live notepad.

## Enrichment Contract

The generation request treats original notes as the user's priority signals and the transcript as supporting context. It asks for concise Markdown with these sections when supported by the conversation:

- Summary
- Key points
- Decisions
- Action items

The prompt requires the model to:

- Preserve every substantive point in the original notes.
- Give extra prominence to issues the user wrote down.
- Treat a later explicit decision as authoritative over an earlier tentative suggestion.
- Mark genuine uncertainty rather than infer a decision.
- Use only property facts present in user context or the transcript.
- Retain any `[SIMULATION]` labels and never present simulated attendees, facts, or decisions as real.
- Omit empty sections instead of inventing content.

The UI keeps **Original** and **Enhanced** views so generated text never overwrites the user's source notes.

## Interface Design

The app closely mirrors the publicly visible Granola desktop interaction without using Granola branding:

- A warm off-white, rounded application canvas.
- A narrow utility sidebar for search, Home, recent sessions, Settings, and New note.
- A centered document column with generous whitespace.
- A large serif meeting title and compact date, attendee, and context pills.
- A borderless sparse-note editor with quiet placeholder text.
- A floating bottom recording control with timer, audio state, and Stop.
- A restrained olive-green progress treatment while analyzing the transcript.
- A completed document that uses subtle headings and bullets instead of cards or dashboard chrome.
- Small, quick transitions for opening a session, starting capture, processing, and switching Original/Enhanced.

The first release includes four visible states: empty library, live meeting, processing, and completed/failed meeting. The layout remains usable with keyboard navigation, visible focus states, semantic controls, and reduced-motion preferences.

## Error Handling and Privacy

- If system-audio permission is denied, keep the session and notes, explain the required macOS setting, and offer Try again.
- If capture initialization or finalization fails, stop cleanly, preserve notes, and show a recoverable error.
- If the OpenAI key is missing or invalid, do not start API processing; send the user to Settings without deleting audio.
- If transcription or enrichment fails, preserve original notes and temporary audio and expose Retry.
- If the app quits during recording, preserve the last autosaved notes and any recoverable audio file.
- The API key stays in Keychain and never appears in session JSON or frontend storage.
- Meeting content has no hosted application backend. Audio and text leave the Mac only for the user's direct OpenAI API requests.
- Successful transcription deletes raw audio by default to control disk usage. Transcript deletion is available from the session menu.

## Distribution

The repository will contain source, an MIT license, a concise README, a build command, and a GitHub Actions release workflow. A tagged release builds one Apple-silicon DMG named `Meeting-Notes.dmg` and attaches it to GitHub Releases.

Because no Developer ID identity is installed, the first release is unsigned. The README and release notes explain the one-time right-click **Open** step required by Gatekeeper. Signing and notarization are the first distribution upgrade once a Developer ID certificate is available; they do not change the app architecture.

## Verification

### Automated checks

- A Rust test round-trips session persistence and proves original notes survive reopen.
- A focused test proves the enrichment input contains original notes, context, transcript, simulation-label rules, and decision-ordering instructions.
- Frontend type checking and a production build catch integration and packaging failures.

### Focused acceptance test

Use a clearly labeled synthetic CRE role-play because no deal packet exists in the repository:

1. Start a meeting titled `[SIMULATION] Harbor Office acquisition call`.
2. Play the role-play through computer audio while typing one sparse note emphasizing the rent-roll concern.
3. State an early tentative suggestion to proceed, then a later explicit decision to pause pending rent-roll verification.
4. Stop the meeting and allow transcription and enrichment to complete.
5. Verify the enhanced notes prominently retain the rent-roll concern, record the later pause as the final decision, and preserve `[SIMULATION]` labels.
6. Quit and reopen the app and verify original and enhanced notes remain available.

No factual property detail is claimed in this test; all people, property details, suggestions, and decisions are labeled simulation state.

## References

- [Tauri architecture](https://v2.tauri.app/concept/architecture/)
- [Apple: Capturing system audio with Core Audio taps](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps)
- [Granola: Writing your own notes](https://docs.granola.ai/help-center/taking-notes/taking-notes-in-granola)
- [Granola: AI-enhanced notes](https://docs.granola.ai/help-center/taking-notes/ai-enhanced-notes)
