# Product workflows implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship a coherent 0.3.0 product release with transcription preferences, transcript tools, grounded meeting questions, and useful recovery actions.

**Architecture:** Extend existing Tauri/Rust commands, protected local JSON persistence, Svelte views, and OpenAI client. Keep audio capture/chunking architecture unchanged.

**Tech stack:** Existing Rust, Tauri, Svelte 5, TypeScript; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-16-product-workflows-design.md`

## Global constraints

- Work only in this existing isolated worktree on `codex/long-recording-hardening`.
- Preserve original notes, source audio/checkpoints, and backward compatibility.
- Never inspect/print/reprocess the recovered lecture's content.
- Never quit, replace, or relaunch the installed app during recording; check immediately before install.
- Run one heavy Rust/Node verification job at a time; clean all owned temporary/build files and processes.
- Existing lower-cost model remains the default; no claims of guaranteed accent recognition or automatic learning.

## Task 1: Native preferences and recovery (backend owner)

- [x] Add failing checks for settings validation/persistence, multipart hints, exact meeting scoping, and explicit no-speech retry.
- [x] Implement settings using existing store protections, extend bootstrap/commands/session defaults, snapshot settings at start, and pass bounded hints/model/language to each chunk request.
- [x] Retry empty checkpoints only on explicit no-speech retry; clear resolved transcription warnings; preserve real progress and notes.
- [x] Reuse ask-meetings with optional exact session ID and validate question length/status.
- [x] Run focused Rust checks; report exact evidence and review diff.

## Task 2: Transcript and question workspace (editor owner)

- [x] Add failing utility/state checks for search, Unicode/literal highlights, safe complete export, and view continuity.
- [x] Extend MeetingEditor with Transcript view, search/copy, Markdown export and original notes preservation. Show available partial transcript on failed/processing meetings.
- [x] Add meeting-specific question form and exact citation excerpts; preserve input on errors and flush current notes before asking/exporting.
- [x] Use incumbent styles and accessible native controls; put new styles in owned component files.
- [x] Run focused frontend checks and report diff.

## Task 3: Settings and recovery continuity (controller)

- [x] Add focused failing recovery/error mapping tests, implement shared structured errors and action decisions.
- [x] Wire settings/bootstrap/API/types and usable settings form with language, model, vocabulary, clear persistence feedback, and specific key errors.
- [x] Improve dock progress/recovery, no-speech guidance, new-note action, key settings return, and cross-view recording readiness.
- [x] Fix library status labels and stale errors; keep errors visible until the next related action succeeds.
- [x] Run focused checks and browser workflow verification.

## Task 4: Review, release, install

- [x] Review combined implementation for privacy, storage safety, request correctness, and UI continuity. Fix material findings.
- [ ] Run full appropriate tests, check, fmt, clippy, production app packaging, and a synthetic live hints check.
- [ ] Bump to 0.3.0, update README/release notes/roadmap and workflow, commit/push/PR/merge/tag without rewriting existing history.
- [ ] Verify public release DMG, then install only if no active recording; verify unchanged user library and exact installed version.
- [ ] Remove owned caches/temporary artifacts, stop all started processes, retain one verified downloadable DMG.
