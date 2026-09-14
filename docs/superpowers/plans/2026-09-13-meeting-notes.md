# Meeting Notes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and publicly release a lightweight macOS app that captures system audio during live note-taking, enriches the user's notes with OpenAI, and restores sessions after restart.

**Architecture:** Tauri 2 hosts a Svelte 5 interface in the system WKWebView. A Rust core owns Core Audio capture, atomic JSON persistence, Keychain access, and direct OpenAI requests; there is no application server or database.

**Tech Stack:** Tauri 2, Svelte 5, TypeScript, Vite, Rust, `objc2-core-audio`/`objc2-audio-toolbox`, `reqwest`, macOS Keychain, OpenAI Responses and Audio Transcriptions APIs.

## Global Constraints

- Support macOS 14.2 and later on Apple silicon.
- Use Tauri 2, Svelte 5, TypeScript, and local CSS.
- Do not use Electron, SwiftUI, ScreenCaptureKit, a bundled browser, a database, a hosted backend, Tailwind, or a component library.
- Capture one source in v1: outgoing macOS system audio. Do not add microphone mixing.
- Do not add prerecorded-audio or transcript upload; every normal session starts in the live notepad.
- Store the user-provided OpenAI API key in macOS Keychain and never expose it to the frontend or session files.
- Use `gpt-4o-mini-transcribe` for transcription and `gpt-6-astra` through `POST /v1/responses` for enrichment, with `store: false`.
- Preserve original notes exactly, keep Original and Enhanced views separate, and retain failed audio for retry.
- Delete raw audio after successful transcription.
- Use an original, independent desktop meeting-notes layout and interactions. Do not use the branding, assets, domains, or identifiers of another meeting-notes product; truthful technology and service names remain allowed.
- Ship an MIT-licensed public repository and one unsigned `Meeting-Notes.dmg`; document the one-time right-click Open step.

## File Map

- `package.json`, `package-lock.json`, `vite.config.ts`, `svelte.config.js`, `tsconfig.json`, `index.html`: frontend build configuration.
- `src/main.ts`: frontend entry point.
- `src/App.svelte`: application state, navigation, and native-event subscription.
- `src/app.css`: complete visual system and responsive desktop layout.
- `src/lib/types.ts`: frontend copies of serialized Rust contracts.
- `src/lib/api.ts`: typed wrappers around Tauri commands plus development-only preview data.
- `src/lib/markdown.ts`: safe parser for the narrow generated Markdown subset.
- `src/lib/markdown.test.ts`: parser checks.
- `src/lib/Sidebar.svelte`: search, session list, New note, and Settings entry.
- `src/lib/MeetingEditor.svelte`: context fields, original/enhanced document, and processing/error states.
- `src/lib/RecordingDock.svelte`: floating capture timer and start/stop controls.
- `src/lib/SettingsDialog.svelte`: Keychain-backed API-key entry.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/build.rs`: Rust package and build inputs.
- `src-tauri/tauri.conf.json`, `src-tauri/Info.plist`, `src-tauri/capabilities/default.json`: macOS bundle, privacy message, and Tauri permissions.
- `src-tauri/src/main.rs`: executable entry point.
- `src-tauri/src/lib.rs`: app construction and managed state.
- `src-tauri/src/domain.rs`: session, status, input, summary, and serializable error types.
- `src-tauri/src/store.rs`: atomic JSON session storage.
- `src-tauri/src/secrets.rs`: macOS Keychain access.
- `src-tauri/src/openai.rs`: transcription, enrichment request construction, response parsing, and Markdown conversion.
- `src-tauri/src/recorder.rs`: Core Audio tap, aggregate device, AAC writer, and single-recorder state.
- `src-tauri/src/commands.rs`: Tauri commands and processing lifecycle.
- `src-tauri/tests/session_store.rs`: persistence and reopen check.
- `src-tauri/tests/openai_contract.rs`: enrichment contract and response conversion checks.
- `src-tauri/tests/lifecycle.rs`: status-transition and recovery checks.
- `src-tauri/icons/icon.svg` and generated icon files: independent Meeting Notes identity.
- `fixtures/simulation-script.txt`: labeled CRE role-play.
- `docs/evaluation.md`: repeatable capture and reopen acceptance test.
- `.github/workflows/release.yml`: tagged Apple-silicon DMG release.
- `README.md`, `LICENSE`, `.gitignore`: public project documentation and repository hygiene.

---

### Task 1: Minimal Tauri and Svelte Shell

**Files:**
- Create: `package.json`
- Create: `package-lock.json`
- Create: `vite.config.ts`
- Create: `svelte.config.js`
- Create: `tsconfig.json`
- Create: `index.html`
- Create: `src/main.ts`
- Create: `src/App.svelte`
- Create: `src/app.css`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/Cargo.lock`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/Info.plist`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/icons/icon.svg`
- Create: generated files below `src-tauri/icons/`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `.gitignore`

**Interfaces:**
- Consumes: macOS 14.2+, Node.js 20+, npm, Apple Command Line Tools, and a minimal stable Rust toolchain.
- Produces: `meeting_notes_lib::build_app() -> tauri::Builder<tauri::Wry>` and frontend scripts `npm run check`, `npm run build`, `npm run tauri`.

- [ ] **Step 1: Install only the missing Rust toolchain**

Run:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
rustup target add aarch64-apple-darwin
```

Expected: `rustc --version` and `cargo --version` succeed without installing full Xcode.

- [ ] **Step 2: Create the smallest build manifests**

Use Tauri package versions compatible with v2, Svelte 5, Vite, TypeScript, and `svelte-check`. Set the bundle identifier to `com.dweng.meetingnotes`, the product name to `Meeting Notes`, the minimum system version to `14.2`, and the active bundle target to `dmg`. Add this privacy message to `src-tauri/Info.plist`:

```xml
<key>NSAudioCaptureUsageDescription</key>
<string>Meeting Notes records computer audio only while you run a meeting.</string>
```

The initial capability grants only core window/event access; filesystem, shell, and HTTP access stay in Rust.

Create a simple independent SVG app icon: a warm-paper rounded square, one dark vertical note line, and one olive recording dot. It must contain no third-party shapes, wordmark, or artwork.

- [ ] **Step 3: Write the failing Rust identity check**

Add to `src-tauri/src/lib.rs` before defining the constants:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_identity_is_stable() {
        assert_eq!(APP_NAME, "Meeting Notes");
        assert_eq!(BUNDLE_ID, "com.dweng.meetingnotes");
    }
}
```

- [ ] **Step 4: Run the check and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml app_identity_is_stable`

Expected: FAIL because `APP_NAME` and `BUNDLE_ID` are undefined.

- [ ] **Step 5: Implement the shell**

Define the constants and builder in `src-tauri/src/lib.rs`:

```rust
pub const APP_NAME: &str = "Meeting Notes";
pub const BUNDLE_ID: &str = "com.dweng.meetingnotes";

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
}
```

Call `meeting_notes_lib::build_app().run(tauri::generate_context!())` from `main.rs`. Render a semantic `<main>` with “Meeting Notes” in `App.svelte`; keep CSS to the base page reset until Task 6.

- [ ] **Step 6: Verify the shell**

Run:

```bash
npm install
npx tauri icon src-tauri/icons/icon.svg
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```bash
git add package.json package-lock.json vite.config.ts svelte.config.js tsconfig.json index.html src src-tauri .gitignore
git commit -m "chore: bootstrap Meeting Notes desktop app"
```

---

### Task 2: Session Domain and Atomic Persistence

**Files:**
- Create: `src-tauri/src/domain.rs`
- Create: `src-tauri/src/store.rs`
- Create: `src-tauri/tests/session_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: application data directory `PathBuf` supplied during Tauri setup.
- Produces: `Session::new(CreateSessionInput) -> Session`; `Session::apply(UpdateSessionInput) -> AppResult<()>`; `SessionStore::new(PathBuf)`; `SessionStore::{list,get,save,delete}` returning `AppResult<T>`.

- [ ] **Step 1: Write the failing persistence check**

Create `src-tauri/tests/session_store.rs`:

```rust
use meeting_notes_lib::domain::{CreateSessionInput, Session};
use meeting_notes_lib::store::SessionStore;

#[test]
fn original_notes_survive_reopen() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "[SIMULATION] Harbor Office".into(),
        context: "All facts are simulation state.".into(),
        attendees: vec!["[SIMULATION] Alex".into()],
    });
    session.original_notes = "rent roll is the issue".into();
    store.save(&session).unwrap();

    let reopened = SessionStore::new(root.clone()).get(&session.id).unwrap();
    assert_eq!(reopened.original_notes, "rent roll is the issue");
    std::fs::remove_dir_all(root).unwrap();
}
```

- [ ] **Step 2: Run the check and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test session_store`

Expected: FAIL because `domain` and `store` do not exist.

- [ ] **Step 3: Implement serializable domain types**

In `domain.rs`, define camel-case serialized types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub title: String,
    pub context: String,
    pub attendees: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionInput {
    pub id: String,
    pub title: String,
    pub context: String,
    pub attendees: Vec<String>,
    pub original_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub context: String,
    pub attendees: Vec<String>,
    pub original_notes: String,
    pub transcript: Option<String>,
    pub enriched_notes: Option<String>,
    pub status: SessionStatus,
    pub error: Option<AppError>,
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus { Draft, Recording, Processing, Complete, Failed }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppError { pub code: String, pub message: String }

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}
```

`Session::new` generates a UUID, an RFC 3339 UTC start time, and uses `Untitled meeting` only when the trimmed title is empty. `Session::apply` uses the ID to update only title, context, attendees, and original notes, so frontend input cannot replace status, transcript, enriched notes, errors, or audio paths.

- [ ] **Step 4: Implement atomic JSON storage**

`SessionStore::save` creates the sessions directory, writes pretty JSON to `<id>.json.tmp`, calls `sync_all`, then renames it to `<id>.json`. `list` reads only `.json` files and sorts newest `started_at` first. `get` rejects IDs containing anything except ASCII letters, digits, or hyphens before constructing a path. `delete` removes that session JSON and its referenced audio file if present.

- [ ] **Step 5: Run persistence checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test session_store
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src src-tauri/tests/session_store.rs
git commit -m "feat: persist meeting sessions atomically"
```

---

### Task 3: Keychain and OpenAI Contracts

**Files:**
- Create: `src-tauri/src/secrets.rs`
- Create: `src-tauri/src/openai.rs`
- Create: `src-tauri/tests/openai_contract.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `Session`, a local `.m4a` path, and an API key loaded inside Rust.
- Produces: `ApiKeyStore::{save,load,exists}`; `OpenAiClient::{validate_key,transcribe,enrich}`; `build_enrichment_request(&Session) -> serde_json::Value`; `sections_to_markdown(EnrichedSections) -> String`.

- [ ] **Step 1: Write the failing enrichment-contract checks**

Create checks that construct a session with `original_notes = "rent roll is the issue"`, a transcript containing an early suggestion to proceed and a later decision to pause, then assert:

```rust
let request = build_enrichment_request(&session);
let encoded = request.to_string();
assert!(encoded.contains("rent roll is the issue"));
assert!(encoded.contains("later explicit decision"));
assert!(encoded.contains("[SIMULATION]"));
assert_eq!(request["model"], "gpt-6-astra");
assert_eq!(request["store"], false);
```

Add a second check proving `sections_to_markdown` omits empty sections and emits `## Decisions` for a nonempty decision list.

- [ ] **Step 2: Run the checks and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test openai_contract`

Expected: FAIL because the OpenAI functions do not exist.

- [ ] **Step 3: Implement Keychain access**

Use `keyring::Entry::new("com.dweng.meetingnotes", "openai-api-key")`. `save` rejects an empty trimmed key, writes it to Keychain, and returns no secret data. `exists` calls `load` and returns a boolean. Map Keychain failures to `AppError { code: "keychain", ... }`.

- [ ] **Step 4: Implement transcription**

`validate_key` sends an authenticated `GET https://api.openai.com/v1/models/gpt-6-astra` and returns `Ok(())` only for a successful response. This request contains no meeting content. `save_api_key` in Task 5 calls validation before writing the key to Keychain.

Use one reusable `reqwest::Client`. POST multipart form data to `https://api.openai.com/v1/audio/transcriptions` with:

```text
model = gpt-4o-mini-transcribe
response_format = json
file = the recorded audio/m4a file
```

Parse `{ "text": "..." }`. Convert 401 to code `invalid_api_key`, 413 to `audio_too_large`, 429 to `rate_limited`, and other non-success responses to `openai` without including the API key in any error.

- [ ] **Step 5: Implement structured enrichment**

POST to `https://api.openai.com/v1/responses` with `model: "gpt-6-astra"`, `store: false`, the complete original notes/context/transcript, and a strict JSON schema for:

```rust
#[derive(Debug, Deserialize)]
pub struct EnrichedSections {
    pub summary: Vec<String>,
    pub key_points: Vec<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
}
```

The instructions explicitly require preserving substantive user notes, prioritizing their emphasis, using a later explicit decision over an earlier tentative suggestion, retaining simulation labels, using only supplied property facts, marking uncertainty, and leaving unsupported sections empty. Parse text content from the completed response, deserialize it, and convert nonempty sections to Markdown.

- [ ] **Step 6: Run contract checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test openai_contract
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src src-tauri/tests/openai_contract.rs
git commit -m "feat: add private OpenAI enrichment pipeline"
```

---

### Task 4: Core Audio System Recorder

**Files:**
- Create: `src-tauri/src/recorder.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Info.plist`

**Interfaces:**
- Consumes: a session ID and output `.m4a` path.
- Produces: `Recorder::new()`; `Recorder::start(&self, session_id: &str, path: &Path) -> AppResult<RecordingInfo>`; `Recorder::stop(&self, session_id: &str) -> AppResult<PathBuf>`; `Recorder::is_recording() -> bool`.

Define the serialized start result exactly once in `recorder.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub session_id: String,
    pub started_at: String,
}
```

- [ ] **Step 1: Write the failing recorder-state check**

Add a pure state check inside `recorder.rs`:

```rust
#[test]
fn a_second_session_cannot_steal_the_recorder() {
    let mut slot = RecordingSlot::default();
    slot.reserve("first").unwrap();
    let error = slot.reserve("second").unwrap_err();
    assert_eq!(error.code, "already_recording");
}
```

- [ ] **Step 2: Run the check and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml a_second_session_cannot_steal_the_recorder`

Expected: FAIL because `RecordingSlot` is undefined.

- [ ] **Step 3: Implement the single-recorder guard**

Store one `Mutex<RecordingSlot>` in `Recorder` and add this deliberate boundary beside it:

```rust
// ponytail: one global recording matches the single-window v1; move to per-session recorders only if concurrent capture becomes a real requirement.
```

`reserve` rejects a second session, and `release` rejects a mismatched session ID.

- [ ] **Step 4: Create the Core Audio tap**

Using `objc2-core-audio` 0.3.x and `objc2-foundation`:

1. Read the default output device and stream UID.
2. Create a private, unmuted, mono `CATapDescription` that captures the global mix while excluding the Meeting Notes process.
3. Call `AudioHardwareCreateProcessTap`.
4. Create a private aggregate device whose tap list contains the tap UUID and has tap auto-start enabled.
5. Read `kAudioTapPropertyFormat` and create an IO proc for the aggregate device.

Every nonzero `OSStatus` becomes an `AppError` containing the operation and numeric status. If any later setup step fails, destroy resources already created in reverse order.

- [ ] **Step 5: Write compressed audio asynchronously**

Using `objc2-audio-toolbox` 0.3.x, create an `ExtAudioFile` with `kAudioFileM4AType` and `kAudioFormatMPEG4AAC`, mono output, and a 48 kbps target. Set the tap format as its client format. Prime `ExtAudioFileWriteAsync` with zero frames before starting the aggregate device, then pass callback buffers to `ExtAudioFileWriteAsync`.

On stop: stop the device, destroy the IO proc, destroy the aggregate device, destroy the tap, dispose the audio file to flush pending writes, and release the slot. Cleanup runs even if an earlier stop call fails.

- [ ] **Step 6: Verify recorder state and compilation**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml recorder
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: PASS on macOS 14.2+.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/Info.plist src-tauri/src
git commit -m "feat: capture macOS system audio"
```

---

### Task 5: Tauri Commands and Processing Lifecycle

**Files:**
- Create: `src-tauri/src/commands.rs`
- Create: `src-tauri/tests/lifecycle.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/domain.rs`

**Interfaces:**
- Consumes: `SessionStore`, `Recorder`, `ApiKeyStore`, and `OpenAiClient` held in `AppState`.
- Produces Tauri commands: `bootstrap`, `create_session`, `save_session(UpdateSessionInput)`, `start_recording`, `stop_recording`, `retry_processing`, `delete_session`, `save_api_key`, `has_api_key`; emits `session-updated` with a complete serialized `Session`.

- [ ] **Step 1: Write failing lifecycle checks**

In `src-tauri/tests/lifecycle.rs`, check pure transition helpers:

```rust
#[test]
fn stop_persists_before_processing() {
    let session = recording_session();
    let stopped = transition_to_processing(session, "/tmp/example.m4a").unwrap();
    assert_eq!(stopped.status, SessionStatus::Processing);
    assert!(stopped.ended_at.is_some());
    assert_eq!(stopped.audio_path.as_deref(), Some("/tmp/example.m4a"));
}

#[test]
fn failed_processing_keeps_original_notes_and_audio() {
    let failed = transition_to_failed(processing_session(), AppError::new("openai", "request failed"));
    assert_eq!(failed.original_notes, "rent roll");
    assert!(failed.audio_path.is_some());
}
```

- [ ] **Step 2: Run the checks and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test lifecycle`

Expected: FAIL because transition helpers are undefined.

- [ ] **Step 3: Implement state and commands**

Define:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub sessions: Vec<Session>,
    pub has_api_key: bool,
}

pub struct AppState {
    pub store: SessionStore,
    pub recorder: Recorder,
    pub keys: ApiKeyStore,
    pub openai: OpenAiClient,
}
```

`bootstrap` returns `{ sessions, hasApiKey }`. Creation and editing save synchronously; editing loads the stored session and applies `UpdateSessionInput` rather than trusting status or file paths from JavaScript. `start_recording` assigns an audio path below the app data directory, starts the recorder, changes status to Recording, and persists. `stop_recording` stops and persists Processing before spawning any API work. `save_api_key` validates the key through `OpenAiClient::validate_key` before storing it.

- [ ] **Step 4: Implement background processing**

The internal async `process_session` loads the key in Rust and transcribes retained audio only when no transcript exists. Immediately after successful transcription, it saves the transcript, deletes raw audio, clears `audio_path`, and persists again before enrichment. It then enriches the session, saves Complete, and emits `session-updated`. A transcription error saves Failed with notes and audio intact; an enrichment error saves Failed with notes and transcript intact. `retry_processing` calls the same path, so retry logic exists once.

- [ ] **Step 5: Register app state and commands**

During Tauri setup, resolve the application data directory, create `SessionStore`, mark stale Recording sessions Failed with code `interrupted`, and manage `AppState`. Register exactly the nine commands named in the Interfaces block.

- [ ] **Step 6: Run lifecycle and full Rust checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test lifecycle
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src src-tauri/tests/lifecycle.rs
git commit -m "feat: connect capture to durable processing"
```

---

### Task 6: Independent Library and Settings UI

**Files:**
- Create: `src/lib/types.ts`
- Create: `src/lib/api.ts`
- Create: `src/lib/Sidebar.svelte`
- Create: `src/lib/SettingsDialog.svelte`
- Modify: `src/App.svelte`
- Modify: `src/app.css`

**Interfaces:**
- Consumes: serialized `Bootstrap`, `Session`, `CreateSessionInput`, `UpdateSessionInput`, and Tauri commands from Task 5.
- Produces: selected session state, filtered session list, Settings modal, New note action, and a development-browser preview of empty and populated libraries.

- [ ] **Step 1: Define exact frontend contracts**

Mirror Rust field names and unions in `types.ts`:

```ts
export type SessionStatus = 'draft' | 'recording' | 'processing' | 'complete' | 'failed';
export type Session = {
  id: string; title: string; startedAt: string; endedAt: string | null;
  context: string; attendees: string[]; originalNotes: string;
  transcript: string | null; enrichedNotes: string | null;
  status: SessionStatus; error: { code: string; message: string } | null;
  audioPath: string | null;
};
export type Bootstrap = { sessions: Session[]; hasApiKey: boolean };
export type CreateSessionInput = Pick<Session, 'title' | 'context' | 'attendees'>;
export type UpdateSessionInput = Pick<Session, 'id' | 'title' | 'context' | 'attendees' | 'originalNotes'>;
```

- [ ] **Step 2: Implement one typed native boundary**

`api.ts` exports one function per registered command and hides `invoke` details. When `window.__TAURI_INTERNALS__` is absent in development, return a fixed `[SIMULATION]` session so the interface can be inspected in a browser; never include real meeting data or an API key.

- [ ] **Step 3: Build the library interaction**

`App.svelte` loads `bootstrap`, selects the newest session, listens for `session-updated`, and updates the session array by ID. `Sidebar.svelte` provides Search, Home, New note, recent sessions, and Settings using semantic buttons and inputs. Search matches title and context locally.

- [ ] **Step 4: Build Keychain settings**

`SettingsDialog.svelte` contains a password input, Save button, close button, a clear explanation that content goes directly to OpenAI, and status text. Saving calls `save_api_key`; the key is cleared from component state immediately after success.

- [ ] **Step 5: Apply the approved visual system**

Use these base tokens in `app.css`:

```css
:root {
  --paper: #f8f7f1;
  --sidebar: #efeee7;
  --ink: #292921;
  --muted: #8b897f;
  --line: #deddd4;
  --accent: #7f9518;
  --accent-soft: #e8edcf;
  --danger: #a24536;
  font-family: Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  color: var(--ink);
  background: #d8d7cf;
}
```

Use a 232px sidebar, a centered document column no wider than 820px, system controls, `Georgia, ui-serif, serif` for meeting titles, 12px radii, thin borders, quiet shadows only for floating controls, visible `:focus-visible`, and reduced-motion overrides. Do not add gradients or decorative illustration.

- [ ] **Step 6: Verify type safety and visual shell**

Run:

```bash
npm run check
npm run build
npm run dev -- --host 127.0.0.1
```

Open `http://127.0.0.1:5173`, capture 1440×1000 and 1100×760 screenshots, compare sidebar width, document measure, typography hierarchy, spacing, controls, and empty space to the approved independent meeting-notes design requirements, then stop the dev server.

- [ ] **Step 7: Commit**

```bash
git add src package.json package-lock.json
git commit -m "feat: add meeting library and private settings"
```

---

### Task 7: Live Notepad, Recording, and Results UI

**Files:**
- Create: `src/lib/MeetingEditor.svelte`
- Create: `src/lib/RecordingDock.svelte`
- Create: `src/lib/markdown.ts`
- Create: `src/lib/markdown.test.ts`
- Modify: `src/App.svelte`
- Modify: `src/app.css`
- Modify: `src/lib/api.ts`
- Modify: `package.json`
- Modify: `package-lock.json`

**Interfaces:**
- Consumes: selected `Session` and typed API wrappers from Task 6.
- Produces: live title/context/attendee/original-note editing, 450ms autosave, capture controls, processing/error UI, Retry, and Original/Enhanced switching.

- [ ] **Step 1: Write the failing Markdown parser check**

Create `src/lib/markdown.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { parseMeetingMarkdown } from './markdown';

describe('parseMeetingMarkdown', () => {
  it('returns safe headings and bullets without interpreting HTML', () => {
    expect(parseMeetingMarkdown('## Decisions\n- Pause deal\n<script>x</script>')).toEqual([
      { kind: 'heading', text: 'Decisions' },
      { kind: 'bullet', text: 'Pause deal' },
      { kind: 'paragraph', text: '<script>x</script>' },
    ]);
  });
});
```

- [ ] **Step 2: Run the check and verify failure**

Run: `npm test -- --run src/lib/markdown.test.ts`

Expected: FAIL because `parseMeetingMarkdown` does not exist.

- [ ] **Step 3: Implement the narrow safe parser**

Split on lines; map `## ` to heading, `- ` to bullet, nonempty remaining lines to paragraph, and discard blank lines. Return text values only and let Svelte escape them. Do not use `{@html}`.

- [ ] **Step 4: Build the editable meeting document**

`MeetingEditor.svelte` renders title, date, attendee and context fields, and a borderless textarea for original notes. On each change, update local state immediately and call `save_session` with `UpdateSessionInput` after 450ms of inactivity; flush before start, stop, session switch, and component destruction. Register Tauri's window close request, prevent the first close, await the flush, then destroy the window. Enhanced mode renders parsed blocks and never mutates `originalNotes`.

- [ ] **Step 5: Build capture and processing states**

`RecordingDock.svelte` shows Start meeting for Draft, a red dot plus monotonic elapsed timer and Stop for Recording, and restrained olive progress for Processing. If `hasApiKey` is false, Start opens Settings instead of beginning capture. Failed sessions show the saved error and Retry when either audio or a transcript exists; only an unrecoverable failed draft returns to Start meeting. Complete sessions default to Enhanced and expose Original/Enhanced segmented controls.

- [ ] **Step 6: Connect native updates**

Start and stop actions disable during their request. The `session-updated` listener replaces the selected session and list item by ID. Permission errors show the exact System Settings path. API errors keep the document editable and leave Retry visible.

- [ ] **Step 7: Verify behavior and visual fidelity**

Run:

```bash
npm test -- --run
npm run check
npm run build
```

Then run the browser preview and inspect Draft, Recording, Processing, Complete, and Failed states at 1440×1000. Confirm keyboard focus, textarea usability, no horizontal scrolling, title hierarchy, bottom control placement, and reduced-motion behavior. Stop the preview server.

- [ ] **Step 8: Commit**

```bash
git add src package.json package-lock.json
git commit -m "feat: add live meeting notepad experience"
```

---

### Task 8: Recovery, Deletion, and Integrated Verification

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/store.rs`
- Modify: `src-tauri/tests/lifecycle.rs`
- Modify: `src/lib/MeetingEditor.svelte`
- Modify: `src/lib/Sidebar.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Consumes: persisted failed/interrupted sessions and retained audio from Tasks 2–5.
- Produces: restart recovery, retry without retranscribing when a transcript exists, session deletion, the new `delete_transcript` Tauri command, and one complete regression command.

- [ ] **Step 1: Extend failing recovery checks**

Add checks proving:

```rust
let recovered = recover_interrupted(recording_session());
assert_eq!(recovered.status, SessionStatus::Failed);
assert_eq!(recovered.error.unwrap().code, "interrupted");

let retry = retry_start(failed_with_transcript());
assert_eq!(retry.status, SessionStatus::Processing);
assert!(retry.transcript.is_some());
```

Add a store check that deletion removes both JSON and the referenced audio file while rejecting an ID containing `../`.

- [ ] **Step 2: Run recovery checks and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test lifecycle --test session_store`

Expected: FAIL on the new recovery and deletion expectations.

- [ ] **Step 3: Implement recovery once at startup**

On bootstrap, convert stale Recording sessions to Failed with a concise recovery message. Retry reuses an existing transcript and only sends audio again when `transcript` is absent. Add and register `delete_transcript(id)`; it clears transcript and enriched notes, changes status to Draft, and leaves original notes untouched. On a normal app exit, stop any active recorder, persist the returned audio path, and mark its session Failed with code `interrupted` so the audio is flushed and retryable.

- [ ] **Step 4: Add destructive-action confirmation in the UI**

The session menu offers Delete transcript and Delete meeting. Each names its exact target in a native-style confirmation panel; Delete meeting also states whether retained audio will be removed. Do not bulk-delete or add a trash system.

- [ ] **Step 5: Run the complete local verification**

Run:

```bash
npm test -- --run
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run tauri build -- --target aarch64-apple-darwin
```

Expected: all commands exit 0 and `src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/Meeting Notes_0.1.0_aarch64.dmg` exists.

- [ ] **Step 6: Commit**

```bash
git add src src-tauri
git commit -m "fix: preserve meetings through failures and restart"
```

---

### Task 9: Evaluation, Public Repository, and Downloadable Release

**Files:**
- Create: `fixtures/simulation-script.txt`
- Create: `docs/evaluation.md`
- Create: `.github/workflows/release.yml`
- Create: `README.md`
- Create: `LICENSE`
- Modify: `.gitignore`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: the verified app and release build from Task 8.
- Produces: repeatable labeled simulation, independent icon, public setup instructions, public GitHub repository, `v0.1.0` tag, and downloadable `Meeting-Notes.dmg` release asset.

- [ ] **Step 1: Add the exact simulation script**

`fixtures/simulation-script.txt` contains only labeled synthetic content:

```text
[SIMULATION] Alex: For the fictional Harbor Office acquisition, I tentatively suggest proceeding this week.
[SIMULATION] Morgan: The rent roll has not been verified, and that affects the underwriting.
[SIMULATION] Alex: Final decision: pause the fictional acquisition until the rent roll is verified. Morgan owns the follow-up for Friday.
```

`docs/evaluation.md` instructs the tester to play this through Mac system audio, type only `rent roll is the issue`, stop, verify the final pause decision, quit, reopen, and verify both Original and Enhanced views remain.

- [ ] **Step 2: Verify the independent bundle identity**

Inspect the Task 1 icon and bundle metadata to confirm they contain no third-party shapes, wordmark, artwork, or bundle identifiers. Regenerate the committed icon sizes from the independent SVG:

```bash
npx tauri icon src-tauri/icons/icon.svg
```

Expected: Tauri generates the configured `.icns` and PNG sizes.

- [ ] **Step 3: Write public documentation**

README sections: What it does, Privacy, Requirements, Download, First launch, Build from source, Focused evaluation, Storage, Limitations, and License. State that macOS 14.2+, Apple silicon, an OpenAI API key with paid API access, and a one-time right-click Open are required. State that v1 captures system audio only and deletes successful raw audio.

- [ ] **Step 4: Add the release workflow**

`.github/workflows/release.yml` uses this shape:

```yaml
name: Release
on:
  push:
    tags: ['v0.1.0']
permissions:
  contents: write
jobs:
  dmg:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: 20, cache: npm }
      - run: npm ci
      - run: rustup target add aarch64-apple-darwin
      - run: npm test -- --run
      - run: npm run check
      - run: cargo test --manifest-path src-tauri/Cargo.toml
      - run: npm run tauri build -- --target aarch64-apple-darwin
      - run: mv "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/Meeting Notes_0.1.0_aarch64.dmg" Meeting-Notes.dmg
      - run: gh release create "$GITHUB_REF_NAME" Meeting-Notes.dmg --title "Meeting Notes $GITHUB_REF_NAME" --notes "Unsigned beta: after copying the app to Applications, right-click it and choose Open once."
        env:
          GH_TOKEN: ${{ github.token }}
```

This produces only `Meeting-Notes.dmg`; do not upload updater archives or signatures.

- [ ] **Step 5: Run the focused local evaluation**

Run the app, grant System Audio Recording permission, play the simulation script through system audio, type the sparse note, stop, and process with a valid user-entered key. Verify:

```text
[x] Original note preserved verbatim
[x] Rent-roll concern prominent
[x] Tentative proceed suggestion not reported as final
[x] Final pause decision present
[x] Simulation labels retained
[x] Raw audio deleted after transcription
[x] Original and Enhanced views survive app restart
```

If no paid API key is available on the test Mac, run all automated request/response contract checks and record the live OpenAI call as the sole unexecuted acceptance item in `docs/evaluation.md`; do not put any key in the repository or shell history.

- [ ] **Step 6: Commit release material**

```bash
git add fixtures docs/evaluation.md src-tauri/icons src-tauri/tauri.conf.json .github README.md LICENSE .gitignore
git commit -m "docs: prepare public Meeting Notes release"
```

- [ ] **Step 7: Perform final verification before publication**

Run:

```bash
git diff --check
npm test -- --run
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run tauri build -- --target aarch64-apple-darwin
git status --short
```

Expected: every check passes and the worktree is clean.

- [ ] **Step 8: Create the public repository without overwriting an existing project**

Run `gh repo view dweng1572179/mac-meeting-notes`. If it does not exist, run:

```bash
gh repo create dweng1572179/mac-meeting-notes --public --source=. --remote=origin --push --description "A private-by-default macOS meeting notepad that captures system audio and enriches sparse notes."
```

If it already exists, stop before changing its remote or contents and report the collision.

- [ ] **Step 9: Tag, release, and verify the one-file download**

```bash
git tag -a v0.1.0 -m "Meeting Notes v0.1.0"
git push origin v0.1.0
gh run list --workflow release.yml --limit 1
gh run watch --exit-status
gh release view v0.1.0 --json url,assets
```

Expected: the workflow passes and the release contains one user-facing DMG asset. Download that asset once, mount it, confirm `Meeting Notes.app` is present, and verify the documented right-click Open path.
