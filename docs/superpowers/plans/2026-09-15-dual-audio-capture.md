# Dual-Audio Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reliably capture and transcribe both macOS system output and the user's microphone without presenting silent recordings as successful meetings.

**Architecture:** Extend the existing Core Audio recorder with a second IO callback on the default input device and keep two temporary AAC files. Persist both paths, transcribe them separately, combine labeled non-empty text, and delete both files only after the combined transcript is durable.

**Tech Stack:** Rust, Core Audio, Audio Toolbox, Tauri 2, Svelte 5, TypeScript, OpenAI Audio Transcriptions API.

**Spec:** `docs/superpowers/specs/2026-09-15-dual-audio-granola-workspace-design.md`

## Global Constraints

- Support macOS 14.2 and later on Apple silicon.
- Keep Tauri 2, Svelte 5, TypeScript, Rust, Core Audio, and the system WKWebView.
- Do not add Electron, SwiftUI, ScreenCaptureKit, a hosted backend, a database, or a bundled browser.
- Keep the OpenAI API key in macOS Keychain and use `store: false` for text generation.
- Preserve both raw audio files after any failed capture-processing step.
- Reject a dual-source recording when both transcriptions are blank.
- Keep older system-only session JSON readable and retryable.

---

### Task 1: Dual-Audio Session Contract and Safe Storage

**Files:**
- Modify: `src-tauri/src/domain.rs`
- Modify: `src-tauri/src/store.rs`
- Modify: `src-tauri/tests/session_store.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Produces: `Session::microphone_audio_path: Option<String>` serialized as `microphoneAudioPath`.
- Produces: `transition_to_processing(Session, system_path, microphone_path) -> AppResult<Session>`.
- Produces: `SessionStore::validate_microphone_audio_path(id, path) -> AppResult<PathBuf>`.

- [ ] **Step 1: Write failing compatibility and deletion checks**

Add tests proving JSON without `microphoneAudioPath` reads as `None`, exact `<id>-mic.m4a` paths save, and deleting a session removes both retained files.

```rust
assert_eq!(legacy.microphone_audio_path, None);
assert!(store.validate_microphone_audio_path(&session.id, &mic_path.to_string_lossy()).is_ok());
store.delete(&session.id).unwrap();
assert!(!system_path.exists());
assert!(!mic_path.exists());
```

- [ ] **Step 2: Run the focused tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test session_store microphone`

Expected: FAIL because the microphone field and path validator do not exist.

- [ ] **Step 3: Implement the minimal backward-compatible fields and validation**

Add `#[serde(default)] pub microphone_audio_path: Option<String>` to `Session`, initialize it to `None`, validate the exact `audio/<id>-mic.m4a` path during save, and remove both validated paths during tombstone cleanup. Mirror `microphoneAudioPath` in the TypeScript `Session` and preview fixtures.

- [ ] **Step 4: Run the focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test session_store`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain.rs src-tauri/src/store.rs src-tauri/tests/session_store.rs src/lib/types.ts src/lib/api.ts
git commit -m "feat: persist dual meeting audio"
```

### Task 2: Native Microphone Recording

**Files:**
- Modify: `src-tauri/src/recorder.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/Info.plist`

**Interfaces:**
- Produces: `RecordingFiles { system: PathBuf, microphone: PathBuf }`.
- Changes: `Recorder::start(session_id, system_path, microphone_path) -> AppResult<RecordingInfo>`.
- Changes: `Recorder::stop(session_id) -> AppResult<RecordingFiles>`.

- [ ] **Step 1: Write the failing recorder-state and command lifecycle checks**

Update command test doubles to return `RecordingFiles`; assert that stop persists both paths and that an interrupted exit keeps both.

```rust
let files = RecordingFiles { system: system_path.clone(), microphone: mic_path.clone() };
let saved = stop_recording_state(&state, &id, |_| Ok(files)).unwrap();
assert_eq!(saved.audio_path.as_deref(), Some(system_path.to_string_lossy().as_ref()));
assert_eq!(saved.microphone_audio_path.as_deref(), Some(mic_path.to_string_lossy().as_ref()));
```

- [ ] **Step 2: Run the focused tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands::tests recorder::tests`

Expected: FAIL because `RecordingFiles` and dual-path signatures do not exist.

- [ ] **Step 3: Extend the existing native owner with a default-input stream**

Reuse the current callback gate, `ExtAudioFile` writer, and Core Audio IOProc. Read `kAudioHardwarePropertyDefaultInputDevice`, its first input stream, and `kAudioStreamPropertyVirtualFormat`; create `audio/<id>-mic.m4a`; start the microphone device after the system tap; and tear down microphone resources before system resources. Map microphone startup failures to `microphone_capture`.

Do not add an AVFoundation or third-party audio dependency.

- [ ] **Step 4: Add the microphone privacy declaration and command paths**

Add:

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Meeting Notes records your microphone while you run a meeting so both sides of the conversation are captured.</string>
```

Have `start_recording` validate and create both exact paths before native startup. Persist both paths only after both recorders are live.

- [ ] **Step 5: Run native unit checks**

Run: `cargo test --manifest-path src-tauri/Cargo.toml recorder::tests commands::tests`

Expected: PASS without requiring live audio hardware.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/recorder.rs src-tauri/src/commands.rs src-tauri/Info.plist
git commit -m "feat: capture microphone with system audio"
```

### Task 3: Non-Empty Dual Transcription and Recovery

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/tests/lifecycle.rs`
- Modify: `src-tauri/tests/openai_contract.rs`
- Modify: `src/lib/RecordingDock.svelte`

**Interfaces:**
- Produces: `combine_transcripts(system: &str, microphone: &str) -> AppResult<String>`.
- Changes: processing transcribes every retained source, persists one labeled transcript, and only then removes audio.

- [ ] **Step 1: Write failing transcript-combination and retention checks**

```rust
assert_eq!(combine_transcripts("Remote words", "My words").unwrap(), "Meeting audio:\nRemote words\n\nYou:\nMy words");
assert_eq!(combine_transcripts("  ", "\n").unwrap_err().code, "no_speech");
assert!(system_path.exists());
assert!(mic_path.exists());
```

Also change `needs_transcription` expectations so `Some("")` is treated as missing.

- [ ] **Step 2: Run focused lifecycle tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test lifecycle`

Expected: FAIL until blank detection and dual retention are implemented.

- [ ] **Step 3: Implement minimal processing**

Transcribe retained paths sequentially, combine only non-empty sources under stable labels, persist the combined transcript, and remove both files after that save succeeds. Return `AppError::new("no_speech", "No speech was detected. Your audio was kept so you can retry.")` when both are blank.

- [ ] **Step 4: Clarify recording and permission UI**

Show `Microphone + computer` in the live dock. Map `microphone_capture` to the macOS Microphone privacy pane and keep the existing system-audio guidance for `audio_capture`.

- [ ] **Step 5: Run backend and frontend checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm test -- --run
npm run check
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/tests/lifecycle.rs src-tauri/tests/openai_contract.rs src/lib/RecordingDock.svelte
git commit -m "fix: reject silent meeting captures"
```

