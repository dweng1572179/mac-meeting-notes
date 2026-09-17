# Focused evaluation

This check uses labeled synthetic content only. It tests that a later final decision overrides an earlier tentative suggestion, while the tester's sparse note and simulation labels remain intact.

## Live run

1. Build and open Meeting Notes on an Apple silicon Mac running macOS 14.2 or later.
2. In OpenAI settings, enter a valid API key with paid API access. Keep the key in the app; do not put it in the repository or shell history.
3. Start a new meeting and grant Microphone and System Audio Recording permission when macOS asks.
4. Type only `rent roll is the issue` in the note editor.
5. While recording, play `fixtures/simulation-script.txt` through Mac system audio. The built-in `say -f fixtures/simulation-script.txt` command is one repeatable way to do this. Also say `[SIMULATION] microphone check from the user` into the microphone.
6. After playback ends, stop the meeting and wait for processing to complete.
7. Check Your notes, AI notes, and Transcript against the checklist below.
8. Quit Meeting Notes, reopen it, select the same meeting, and check all three views again.
9. Set its folder to `Acquisitions`, return to that folder, and ask `What was the final decision and what remains open?`. Confirm the answer cites this meeting and the citation opens it.
10. In Finder, choose **Go → Go to Folder**, enter `~/Library/Application Support/com.dweng.meetingnotes/audio/`, and confirm the completed meeting has no retained `.m4a` file there.

```text
[ ] Your typed note preserved verbatim
[ ] Rent-roll concern prominent
[ ] Tentative proceed suggestion not reported as final
[ ] Final pause decision present
[ ] Simulation labels retained
[ ] System audio and Microphone transcript labels contain speech with source-relative offsets
[ ] Folder question links to an exact meeting excerpt
[ ] Successfully transcribed source sections cleaned after durable checkpoints
[ ] Your notes, AI notes, and Transcript survive app restart
```

## v0.2.7 automated and live synthetic acceptance

The ignored `live_synthetic_long_recording_with_openai` test generates speech with macOS `say`, encodes AAC using the system audio tool, and invokes the actual native chunking, OpenAI, and atomic persistence pipeline against a disposable local library. It does not record the microphone or use existing meetings. Explicitly run it with paid API access and the existing login Keychain key:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib live_synthetic_long_recording_with_openai -- --ignored --nocapture --test-threads=1
```

Verified on September 16, 2026: 340.4 seconds, two chunks, 1,224 transcript words, correct final decision in enhanced notes, original notes unchanged, completed session reopened, and no retained synthetic audio.

The ordinary offline suite independently decodes all chunks from a 605.25-second source and checks duration/content and upload size. Local HTTP fixtures exercise a failed middle chunk followed by restart and resumed uploads, storage failure before checkpoint, final-checkpoint interruption cleanup, empty system audio, silent microphone, and a corrupt source alongside a healthy one. Frame-counter tests distinguish silence, missing frames, empty callbacks, stalls, write errors and short capture. These tests require no microphone permission or spoken input.

## Physical capture check

The optional physical-device run above checks a particular microphone and output-device setup. Synthetic acceptance covers the long-recording processing and storage boundaries without collecting private meeting content.

The source tree has the following automated evidence:

- OpenAI request/response contracts cover transcription, enrichment, `store: false`, source-bounded meeting questions, and rejection of invented citations.
- Frontend/API checks cover autosave, close/save ordering, exact folder filtering, the meeting-question boundary, and restart-facing state.
- Recovery and storage checks cover interrupted recording/processing, transcript-preserving retry, dual-audio deletion, exact-path validation, and deferred cleanup.
- Native recorder checks cover single-owner recording, safe callback shutdown, microphone error mapping, and preservation of both audio paths for retry.

These checks do not substitute for verifying the selected physical microphone before an important meeting.

## v0.3.0 product workflow acceptance

Verified on September 16, 2026:

- 104 ordinary Rust checks passed, with the paid live check excluded from the ordinary suite; all 40 frontend/API checks passed.
- Svelte check reported zero errors/warnings; production frontend build, Rust formatting, Clippy with warnings denied, and diff whitespace checks passed.
- The paid synthetic check passed separately using `gpt-4o-transcribe`, English, and bounded vocabulary hints: 414.7 seconds, two chunks, 922 transcript words, correct final decision, preserved original notes, successful reopen, and cleaned synthetic audio.
- The speech fixture now selects macOS Samantha explicitly: this machine's default `say` voice returned success with only 0.005 seconds of audio. The fixture validates duration before making API calls.
- Browser checks exercised persisted preferences, literal transcript highlighting, question pending-state continuity, no-speech recovery, and layouts at 1280×900 and 800×720 without horizontal overflow. Recovery appears inline above notes.
- Review findings were fixed and rechecked: pending questions cannot be edited, exports label incomplete transcripts, and questions use complete sources or explicitly reject a selection over 200,000 UTF-8 bytes instead of silently cutting source text.

This evidence covers workflow correctness and the configured recognition request. It does not establish accuracy across all accents, Bluetooth reconnection, or uninterrupted multi-hour physical capture. Existing local meeting content was not used in the tests.

### Published artifact and installed-app verification

[Release v0.3.0](https://github.com/dweng1572179/mac-meeting-notes/releases/tag/v0.3.0) was built from `fbfd321d4547c91b4024ffe504e0b4f534a445a3`. [Release workflow 35176722703](https://github.com/dweng1572179/mac-meeting-notes/actions/runs/35176722703) passed all checks, production compilation, packaging regression checks, and mounted signature/entitlement verification.

- DMG: 5,944,097 bytes; SHA-256 `55fdb81f8eed42f6fd0eadf4b1e2b8f9aebb1aa5229e6dd6f7036316abe83877`.
- The downloaded public artifact passed a second local digest check, read-only mount, strict signature verification, version check, and audio-input entitlement check.
- Installation occurred with the app stopped and no recording/processing session. The installed executable matched the public artifact exactly.
- Native smoke testing verified synthetic note autosave, Markdown export including Unicode text and draft status, recognition settings, and successful reopening. The disposable note/export were removed, and the existing library remained byte-for-byte unchanged.
- Generated build caches, verification screenshots, mounts, temporary artifacts, and the rollback app were removed. One verified DMG remains in Downloads; no development servers, test jobs, or browser automation sessions remain running.

## v0.4.0 live workspace acceptance

The new release adds recording-time transcription of roughly 60-second finalized sections, speaker detection, editable AI notes, and grounded suggestions. The checklists below are acceptance procedures, not a claim that all release checks have run. Keep the dated v0.2.7 and v0.3.0 evidence above as historical results.

### Automated evidence recorded during implementation

On September 17, 2026, the focused native recorder suite passed **30 tests**. Synthetic native AAC tests covered:

- Separate source sections and final tails with decoded tones, precise durations, and cumulative frame totals.
- Callback/rotation overlap preserving exactly 67,200 decoded frames and 4.2 seconds of source-relative timeline.
- Concurrent stop/rotation publishing each source frame once and releasing the recorder slot.
- Disposal waiting on an admitted callback lease, on the control thread only.
- Existing-path collisions preserving prior files while the healthy source continues.
- A short finalized file retained with warnings instead of being published as complete.
- Failed codec preparation cleaning up only its owned, unwritten file so the next rotation can recover.

The partial-finalization and preparation-retry regression tests each failed before their fixes and passed afterward. These tests used temporary synthetic files, no audio devices, and no existing meetings. Broader backend/frontend, paid speaker-detection acceptance, packaging, and installed-app results must be recorded only after their respective runs finish.

### Live processing and recovery

Use a disposable library and a local fixture server for failures. A synthetic recording longer than two sections should demonstrate:

```text
[ ] A transcript checkpoint appears while session status remains Recording
[ ] Only one section upload is in flight; slow HTTP does not delay capture rotation
[ ] Both source tracks retain their correct section/offset references
[ ] Stop finalizes tails once and drains pending work before generating AI notes
[ ] Only one structured enrichment request produces notes and suggestions
[ ] Network failure pauses transcription but leaves capture running and audio retained
[ ] Restart/retry skips saved results and recovers unregistered finalized sections
[ ] A corrupt or partially readable section does not discard healthy-source progress
[ ] Inserting a recovered earlier section cannot misassign an in-flight response
[ ] Startup failure leaves captured sections recoverable rather than blocking every later start
[ ] Retained/corrupt sections are never silently erased during error recovery
```

### Recognition, note ownership, and suggestions

Use labeled simulation speech with an explicit speaker introduction and a separately mentioned nonparticipant. New preferences should choose Speaker detection; an existing saved text-only preference should stay unchanged.

```text
[ ] Speaker detection requests timed turns and omits unsupported vocabulary prompts
[ ] Equal speaker labels in different source/uploads remain different scoped identities
[ ] Raw words remain available when speaker-turn text differs from the raw transcript
[ ] Suggestions contain exact saved-source excerpts and valid source references
[ ] A mentioned name is not automatically accepted as a participant
[ ] Accept/dismiss is explicit and unrelated manual fields remain unchanged
[ ] Your notes remain verbatim, including a correctly explained empty typed-notes view
[ ] AI-note edits survive refresh/retry/restart separately from the generated baseline
[ ] Restore generated requires an explicit action
[ ] Refreshing a completed legacy text meeting adds notes/suggestions, not invented speakers
[ ] Enrichment failure preserves transcript checkpoints and user edits
```

Speaker detection and metadata suggestions remain fallible even with valid evidence references. Review the actual excerpts and attribution; tests of request formatting are not an accent or speaker-recognition benchmark.

### Workspace and physical-device checks

Verify desktop and compact layouts, keyboard access, reduced-motion preferences, transcript search/copy/export, topic links, and expandable exact question sources. Command/Ctrl+Enter submits the meeting composer; empty or pending submissions must not start another request. Check the same question behavior wherever the composer is reused.

For a physical-device check, select the intended microphone/output before starting a disposable simulation. Do not replace or quit over an existing real recording. Bluetooth reconnect/rebinding, privacy-identity stability across ad-hoc updates, and uninterrupted multi-hour physical capture remain limitations. Native synthetic rotation does not establish those hardware guarantees.

### Paid synthetic acceptance

The older paid long-recording test above remains available and its results remain historical. The new ignored `live_synthetic_segmented_diarization_with_openai` test generates fresh Samantha speech, requires it to cross a 60-second section boundary, and exercises the speaker-detection response, exact evidence validation, one combined enrichment request, durable progress, and cleanup. It uses the authorized login Keychain key and a disposable synthetic library; it does not open existing meetings or record a physical device.

```sh
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib live_synthetic_segmented_diarization_with_openai -- --ignored --nocapture --test-threads=1
```

On 2026-09-17, the paid fixture passed with 105.4 seconds of freshly generated speech, 2 finalized sections, and 41 speaker turns. It verified a pre-stop checkpoint, the final Monday decision, a grounded title suggestion, durable notes on reopen, and section cleanup. The test completed in 93.36 seconds; this is a sequential synthetic acceptance run, not a measurement of end-of-meeting wait time. A single synthetic voice exercises the diarized response path, not real-world multi-speaker recognition accuracy. API charges depend on the selected models, both audio tracks, and any extra questions/refreshes/retries; do not infer an exact per-meeting price from this fixture.


### Review and UI evidence — 2026-09-17

Independent review and regression checks covered concurrent section registration, corrupt sections with healthy-source progress, failed startup recovery, uploader ownership through terminal persistence, complete section coverage before cleanup, and exact participant-name evidence. The real API exercise exposed all-or-nothing metadata validation; unsupported suggestions are now omitted individually while notes and verified suggestions remain available.

Synthetic browser checks at desktop and compact sizes covered note tabs, empty typed notes, transcript topics, literal search, question sources, suggestion acceptance/dismissal, edited AI notes surviving navigation, and restore followed immediately by navigation/export. API calls and UI helpers passed 57 frontend tests; type diagnostics reported zero errors/warnings and the production frontend built successfully. Browser sessions and preview servers were closed.

Real microphones, Bluetooth reconnection, accents, and long physical meetings were not part of this synthetic acceptance.

Final local verification on 2026-09-17 passed 139 Rust tests (2 paid tests remain opt-in), all-target Clippy with warnings denied, Rust formatting, and diff whitespace checks. The new paid segmented test was separately run and passed as recorded above.
