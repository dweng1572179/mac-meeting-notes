# Recording and notes reliability implementation plan

**Goal:** Fix reproduced recovery and content-loss failures, then evaluate representative bilingual notes quality.

**Architecture:** Keep the existing native recorder, single uploader, one-minute sections and atomic checkpoints. Treat optional speaker annotations separately from required transcript text. Reuse existing tests and APIs without dependencies or a realtime rewrite.

**Constraints:** Preserve local meetings; never inspect or reprocess the protected lecture. Never replace a running recording. No new runtime dependencies. Run heavy checks sequentially. Public evidence uses synthetic data only.

- [x] API boundary: regress malformed optional speaker metadata and truncated response bodies; keep usable words, classify transport failures separately.
- [x] Recovery: bound transient retries, preserve checkpoints, continue healthy sections after a section-specific rejection, retain incomplete status and failed audio.
- [x] Content: canonical words take precedence over disagreeing speaker annotations in display and enrichment; unchanged generated notes are not independent evidence on refresh.
- [x] Editor: synchronize clean drafts after generation, preserve dirty drafts, announce completion only after completion.
- [x] Capture: keep accumulated missing-duration warnings visible after callbacks resume. Do not claim device switching or wall-clock alignment is solved.
- [x] Notes quality: preserve concrete details and uncertainty, avoid business decisions/actions for conversations that contain none. Run a paid synthetic bilingual evaluation within a small bounded budget.
- [x] Verify Rust/frontend regressions and review; document both proven fixes and remaining physical-device/recognition limits.
- [x] Release and install only after checking recording inactivity again.

Review focus: transient errors during recording and stopping; malformed/null metadata with valid required text; pending poisoned section beside healthy sections; focused clean versus dirty notes; bilingual facts, negations, changed decisions and genuinely absent action items.

The current saved meeting is complete. The earlier confirmed incident stopped uploads at 8/32 sections because invalid speaker timestamps were fatal. Its recovered text is retained; this audit does not imply a new capture failure has been reproduced.
