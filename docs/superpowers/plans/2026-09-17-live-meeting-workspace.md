# Live meeting workspace implementation plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Read the spec and your task ownership before editing. Keep all heavy checks sequential.

**Goal:** Release a live, readable, assistive meeting workspace as 0.4.0.

**Architecture:** Native section rotation and one durable sequential uploader; backward-compatible JSON fields; existing Responses call generates notes and grounded suggestions; Svelte views preserve the current visual identity.

**Tech stack:** Existing Rust, Core Audio/AudioToolbox, Tauri, Svelte/TypeScript. No new runtime dependencies.

**Spec:** `docs/superpowers/specs/2026-09-17-live-meeting-workspace-design.md`

## Global constraints

Preserve all existing notes and raw evidence. No installed-app mutation during recording. Work only in this worktree. Do not read or reprocess the protected lecture. Do not upload user test content during automated tests. Use synthetic audio/content. One heavy Rust/Node check at a time; no watches left running. User authorization covers implementation, release, and safe installation.

## Task 1: Safe native rotation (native owner)

Files: `src-tauri/src/recorder.rs`, `src-tauri/src/audio.rs` and focused native test modules only.

- [ ] Add failing checks for source sections, boundary frame continuity, retirement safety, and final-tail publication.
- [ ] Add `Recorder::start_segmented`, `Recorder::rotate`, and finalized `CapturedSegment` results. Stable per-section sink pointers/gates, off-callback disposal, cumulative health. Preserve the legacy start path through the same shared setup.
- [ ] Use derived `<id>-<system|mic>-segment-<8-digit-index>.m4a` paths. Publish only finalized valid sections, retain unreadable files. Finalization errors cannot discard captured frames.
- [ ] Run native checks when granted the Rust test slot; report exact interfaces/evidence.

## Task 2: Recognition and grounded enrichment (backend owner)

Files: `src-tauri/src/domain.rs`, `src-tauri/src/openai.rs`, `src-tauri/tests/openai_contract.rs`, `src-tauri/tests/lifecycle.rs`.

- [ ] Add failing HTTP/schema checks for diarization settings/turn validation, supported fast model, grounded suggestions/topic anchors, and legacy compatibility.
- [ ] Extend `TranscriptChunk` with defaulted `segment_index: Option<u64>` and `segments: Vec<TranscriptSegment>`; return a typed transcription result. Add defaulted session capture manifest/live error/suggestion/edit fields coordinated with parent.
- [ ] Use diarized_json/auto chunking without unsupported prompts; keep text-model behavior and output-limit safeguards. Keep legacy default mini and expose distinct new-recording speaker default.
- [ ] Extend one enrichment response with validated suggestions/topics, retain notes Markdown, switch to the pinned mini model. Exact evidence and source-key validation reject invented metadata.
- [ ] Run focused checks; parent integrates command persistence. Do not edit commands/store/UI files.

## Task 3: Clear and polished workspace (editor owner)

Files: `src/lib/MeetingEditor.svelte`, `TranscriptView.svelte`, `MeetingQuestion.svelte`, `LibraryView.svelte`, `meeting-workspace.ts`, associated frontend checks, and scoped `src/app.css` edits.

- [ ] Add failing helper/state checks for view clarity, speaker scope, chronological paragraphs, topic anchors, and display/export of edited AI notes.
- [ ] Implement explicit note labels/empty states, tighter title sizing, compact capture disclosure, structured transcript, raw access, and adjacent accept/dismiss suggestions using agreed callbacks/API.
- [ ] Replace question forms with compact composers, per-view exchange history, rendered Markdown, collapsible citations, keyboard controls and reduced-motion-compatible native transitions.
- [ ] Add AI-note editing without changing manually typed notes; preserve pending autosave/close behavior.
- [ ] Run scoped frontend checks after dependencies are ready; parent owns shared API/types/settings/App.

## Task 4: Ongoing processing, persistence, settings (controller)

Files: `src-tauri/src/commands.rs`, `store.rs`, `processing_tests.rs`, `tests/session_store.rs`, `lib.rs`; `src/lib/api.ts`, `api.test.mjs`, `types.ts`, `SettingsDialog.svelte`, `RecordingDock.svelte`, `recovery.ts`, `src/App.svelte`.

- [ ] Add failing checks for safe section paths/reconciliation, durable before-stop transcripts, API failure while recording, single worker, final-tail completion, retry and section cleanup.
- [ ] Implement separate rotation supervisor and single uploader with session transaction boundaries; queue emptiness while recording never completes/deletes an active source. Stop joins the same processing owner.
- [ ] Persist suggestions and edited notes with focused commands; refresh completed notes from retained text without replaying transcription; merge into latest session and preserve manual edits.
- [ ] Wire types/API/settings, new-recording diarization default, live status/error details, and existing-data compatibility. No unsupported vocabulary hints in speaker mode.

## Task 5: Review and delivery (controller/reviewer)

- [ ] Review concurrency/storage/API behavior and screenshots; fix material findings and obtain scoped re-review.
- [ ] Run full relevant suites, formatting, Clippy, frontend production build, browser workflows, synthetic live API check, and native packaging verification.
- [ ] Document limitations and costs; bump 0.4.0; commit/push/PR/merge/tag; verify public CI artifact and install only while inactive.
- [ ] Verify native app workflows and unchanged real library; clean owned caches/artifacts/processes and retain one verified DMG.
