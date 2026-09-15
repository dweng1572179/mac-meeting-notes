# Local Meeting Intelligence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Granola 2.0-style local meeting timeline, lightweight folders, and source-linked AI questions without adding accounts, sync, or a server.

**Architecture:** Store one optional folder label directly on each existing session JSON. Derive navigation and timelines in Svelte, and add one Rust command that sends a bounded set of selected meeting sources to OpenAI and validates returned citations before rendering them.

**Tech Stack:** Svelte 5, TypeScript, local CSS, Rust, Tauri 2, OpenAI Responses API.

**Spec:** `docs/superpowers/specs/2026-09-15-dual-audio-granola-workspace-design.md`

## Global Constraints

- Keep the product single-user, local-first, and independently branded.
- Do not add a database, embeddings, cloud sync, accounts, team sharing, templates, Slack, drag-and-drop, or another AI provider.
- Keep meeting notes and transcripts as the durable source of truth.
- Folder chat is ephemeral and uses the user's Keychain-backed OpenAI key with `store: false`.
- AI citations must reference an eligible session and an exact excerpt from content supplied to the model.

---

### Task 1: Folder Labels and Library Navigation

**Files:**
- Modify: `src-tauri/src/domain.rs`
- Modify: `src-tauri/tests/session_store.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`
- Modify: `src/App.svelte`
- Modify: `src/lib/MeetingEditor.svelte`
- Modify: `src/lib/Sidebar.svelte`
- Create: `src/lib/LibraryView.svelte`
- Modify: `src/app.css`

**Interfaces:**
- Produces: `Session::folder: String` with `#[serde(default)]`.
- Changes: `UpdateSessionInput::folder: String`.
- Produces: `LibraryView` props `{ sessions, folder, hasApiKey, onSelect, onOpenSettings }`.

- [ ] **Step 1: Write failing compatibility and editor-state checks**

Add a Rust assertion that legacy JSON defaults `folder` to an empty string and a frontend state check proving folder filtering is exact and deterministic.

- [ ] **Step 2: Run focused checks and verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test session_store legacy
npm test -- --run
```

Expected: FAIL because `folder` and the filter helper do not exist.

- [ ] **Step 3: Add the folder field end to end**

Initialize legacy/new sessions with an empty folder, include it in autosave, and render a quiet Folder input beneath Attendees and Context.

- [ ] **Step 4: Build the derived sidebar and timeline**

Derive sorted unique non-empty folder names from sessions. `All meetings` clears the selected meeting and folder; a folder button opens `LibraryView` filtered to that exact label. The timeline shows title, attendees/context, date, and status without duplicating session data.

- [ ] **Step 5: Apply the reference hierarchy**

Use the current warm paper palette and independent wordmark. Add a wide split workspace, compact timeline rows, restrained selection surfaces, visible focus rings, and responsive collapse below 900px. Do not copy Granola assets or branding.

- [ ] **Step 6: Run frontend checks**

Run:

```bash
npm test -- --run
npm run check
npm run build
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domain.rs src-tauri/tests/session_store.rs src/App.svelte src/app.css src/lib
git commit -m "feat: add local meeting folders"
```

### Task 2: Source-Validated Folder Questions

**Files:**
- Modify: `src-tauri/src/domain.rs`
- Modify: `src-tauri/src/openai.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/openai_contract.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Produces: `MeetingAnswer { answer: String, citations: Vec<MeetingCitation> }`.
- Produces: `MeetingCitation { session_id: String, title: String, excerpt: String }`.
- Produces: Tauri command `ask_meetings(folder: Option<String>, question: String) -> AppResult<MeetingAnswer>`.

- [ ] **Step 1: Write failing request and citation-validation checks**

Construct two sessions with distinct source tokens. Assert the request contains only eligible selected-folder sources, uses `gpt-6-astra`, and sets `store` to false. Return one valid exact excerpt and one invented excerpt; assert only the valid citation survives.

- [ ] **Step 2: Run the contract tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test openai_contract folder_question`

Expected: FAIL because the request and answer types do not exist.

- [ ] **Step 3: Implement the bounded source request**

Select the most recent 20 completed meetings with non-empty notes/transcript/enrichment. Supply at most 8,000 UTF-8 characters per meeting, with ID, title, date, context, notes, transcript, and enhanced notes. Use a strict JSON schema for `answer` and citation `{session_id, excerpt}` records.

- [ ] **Step 4: Validate the response at the native boundary**

Reject unknown session IDs and excerpts that are not exact substrings of the corresponding supplied source. Attach trusted titles from local session data rather than accepting model-provided titles.

- [ ] **Step 5: Add and register the Tauri command**

Reject blank questions, load the key from Keychain, filter by exact folder when supplied, and return a direct error when no eligible source material exists. Do not hold the session mutex across the network request.

- [ ] **Step 6: Run backend checks**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domain.rs src-tauri/src/openai.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/tests/openai_contract.rs src/lib/types.ts src/lib/api.ts
git commit -m "feat: ask meetings with verified sources"
```

### Task 3: Granola-Style Ask Panel

**Files:**
- Modify: `src/lib/LibraryView.svelte`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/app.css`
- Test: `src/lib/api.test.mjs`

**Interfaces:**
- Consumes: `askMeetings(folder, question) -> Promise<MeetingAnswer>`.
- Produces: an accessible question form, answer region, and citation buttons that call `onSelect(sessionId)`.

- [ ] **Step 1: Write the failing API wrapper check**

Assert the native wrapper invokes `ask_meetings` with `{ folder, question }` and the preview path returns a deterministic source-linked simulation response.

- [ ] **Step 2: Run the focused test and verify failure**

Run: `node --disable-warning=ExperimentalWarning --experimental-strip-types --test src/lib/api.test.mjs`

Expected: FAIL because `askMeetings` does not exist.

- [ ] **Step 3: Implement the ask panel**

Add one textarea/input, a submit button, pending state, inline error, answer copy, and source cards. If no key exists, open Settings. Selecting a source opens its meeting. Keep prior answers only in component memory.

- [ ] **Step 4: Verify accessibility and responsive states**

Ensure the form has a visible label, pending controls are disabled, errors use `role="alert"`, answers use `aria-live="polite"`, source cards are buttons, and the split panel stacks below 900px.

- [ ] **Step 5: Run all frontend checks**

Run:

```bash
npm test -- --run
npm run check
npm run build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lib/LibraryView.svelte src/lib/api.ts src/lib/types.ts src/app.css src/lib/api.test.mjs
git commit -m "feat: add source-linked meeting questions"
```

### Task 4: Release Hardening and Documentation

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `README.md`
- Modify: `docs/evaluation.md`

**Interfaces:**
- Produces: public release version `0.2.0` and one Apple-silicon `Meeting-Notes.dmg`.

- [ ] **Step 1: Update public product truth**

Document dual capture, the one-time Microphone and System Audio permission prompts, local folder behavior, source-linked questions, direct OpenAI data flow, and retained-audio recovery. Remove the v1 claim that microphone capture is unsupported.

- [ ] **Step 2: Bump every package version to 0.2.0**

Keep `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json` synchronized.

- [ ] **Step 3: Run complete verification**

Run:

```bash
npm test -- --run
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

Expected: every command exits 0.

- [ ] **Step 4: Commit**

```bash
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json README.md docs/evaluation.md
git commit -m "chore: prepare Meeting Notes 0.2.0"
```

- [ ] **Step 5: Publish and verify**

Push the branch, merge to `main`, tag `v0.2.0`, wait for the release workflow, download the DMG to a temporary directory, verify its SHA-256 digest and strict nested app signature, then delete the temporary verification files.

