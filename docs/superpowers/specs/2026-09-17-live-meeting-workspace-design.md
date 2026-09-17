# Live meeting workspace

The user wants useful meeting intelligence as the meeting happens, clear note ownership, readable speaker turns, editable AI notes, grounded suggestions, and a question interface that fits the existing paper/olive visual system.

## Processing and capture

Native capture rotates independently valid AAC sections around every 60 seconds on a control task, never inside an audio callback. A separate, single-owner processor transcribes finalized sections sequentially while capture continues. Successful text/turns are atomically checkpointed before section audio can be removed. Network failure pauses transcription, retains audio, and leaves recording running. Stop finalizes tails; only then may the drained worker generate notes. No repeated whole-transcript summarization during recording.

Capture uses strictly derived numbered section paths and a durable section manifest. Startup reconciles unregistered section files after interruption. Unreadable tails stay available with an explicit warning. Legacy whole-file recordings keep their existing retry path. Callback retirement and disposal remain safe under concurrent rotation/stop; capture health stays cumulative.

## Recognition and intelligence

New/unset preferences default to speaker detection; explicitly saved preferences and missing settings on old sessions preserve their prior meaning. The supported diarization API returns timed turns. Speaker identities are scoped to each source/upload; matching labels in different uploads are not assumed to identify the same person. Vocabulary prompts are unavailable in this mode and the UI explains this. Raw text remains preserved.

Use one `gpt-4.1-mini-2025-04-14` structured response for the existing notes plus suggested title, context, category, participants, and transcript topic anchors. Suggestions require exact supporting excerpts and valid source references. Merely mentioned people are not participants. Transcript organization reuses all source words and adds grounded topic navigation and paragraphs; it does not rewrite or silently shorten evidence.

Users explicitly accept or dismiss suggestions. User fields and typed notes are never overwritten automatically. Edited AI notes live separately from the generated baseline and remain editable/preserved across retries. Existing completed recordings can request refreshed notes/suggestions using saved text; genuine speaker attribution requires retained audio and is never invented retrospectively.

## Workspace

Preserve the incumbent paper/olive/Georgia identity. Rename views to `Your notes`, `AI notes`, and `Transcript`; explain each briefly, including an honest empty typed-notes state. Default completed meetings to AI notes. Reduce header whitespace, retain editable title/category/participants, and show suggestions beside their fields.

Replace the heavy question form with a compact composer, keyboard submission, a modest send affordance, readable answer history, and collapsible exact sources. Reuse this treatment for library questions. Use Svelte/CSS motion with reduced-motion support; do not import React/Framer solely for animation.

Render speaker turns in chronological source-relative order, with topic navigation and compact timestamps. Keep raw text accessible. Legacy text gets readable paragraphs without fabricated speakers. Capture details become a compact disclosure, with actionable live failures still visible.

## Safety and acceptance

Work only in the existing worktree/branch. Preserve the protected lecture and current TEST recording byte-for-byte unless the user explicitly invokes a refresh/edit in the app. Never install or quit over a recording. Keep native dependencies unchanged and heavy checks sequential on the 8 GB host.

Verify native synthetic rotation boundaries, safe retirement, interruption/retry, one live uploader, pre-stop checkpoints, failure without stopping capture, final-tail completion, durable suggestions/edits, source validation, empty states, speaker scoping, copy/export, keyboard/reduced-motion behavior, desktop/compact UI, and one paid synthetic acceptance. Build/publish/install a reviewed 0.4.0 release under existing authorization, then remove owned caches and disposable data.
