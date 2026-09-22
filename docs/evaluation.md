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


### Published artifact and installation — 2026-09-17

[Release v0.4.0](https://github.com/dweng1572179/mac-meeting-notes/releases/tag/v0.4.0) was built from `744d25e824c85b19783afd041f50ad5307425ac1`. [Release run 35270801673](https://github.com/dweng1572179/mac-meeting-notes/actions/runs/35270801673) passed frontend tests/type checks, Rust tests, all-target Clippy, the production app build, packaging regression, and mounted-app signature verification.

- Public DMG: 6,147,400 bytes; SHA-256 `e34411340b411975a7f36a9335d2c79d5f7417ae1ec742eaa963c253c6ff9631`, matching GitHub's asset digest.
- The downloaded artifact passed a read-only mount, strict deep signature verification, version 0.4.0 check, and audio-input entitlement check.
- Installation followed a fresh inactive-session check and graceful quit. Every installed bundle file matched the public artifact; executable SHA-256 is `d2879deab269f0e330252e0e08523deb3d8504e8f455eae225d949c748812360`.
- Both existing meeting files remained byte-for-byte unchanged. Neither was reprocessed. One verified release DMG remains in Downloads.
- Native launch reached the app window, but the remaining native UI smoke check is **pending user Keychain approval**. A targeted process sample showed the synchronous bootstrap waiting inside macOS Keychain access. UI inspection timed out. This is not recorded as a passing native smoke test; the synthetic browser acceptance above remains the verified UI evidence. No Keychain permissions or credentials were changed by the agent.

The paid fixture, production build, and automated checks establish the tested software paths. They do not remove ad-hoc-signing prompts or establish physical-device, accent, or multi-speaker accuracy.


## v0.4.1 presentation corrections — 2026-09-17

The reported composer defect came from overlapping textarea and wrapper focus rules, duplicated across two implementations. Both views now share one compact composer. Repeated “Transcript section” headings exposed internal upload boundaries; plain chunks now show source/time metadata and paragraph text without that heading.

Regression tests first reproduced repeated chunk headings, long unbroken text, and unsupported Markdown headings. The final checks cover source-text retention, cross-paragraph phrase search, legacy-only boilerplate filtering, scoped speakers, and escaped inline Markdown. Independent review caught and verified the search, boilerplate, and speaker-scope edge cases before release.

Synthetic browser checks at 1180, 900, and 720 pixels verified a single wrapper focus border with no textarea outline/resize handle, no horizontal overflow, growing long questions, Command+Enter, duplicate-submit protection, a retained question after a failed request, successful retry/clear, safe emphasis rendering, expandable exact sources, and phrase matches across display breaks. No real meeting text was used in these checks. The mechanical design scan found only existing global font/capture-warning styling; the patch preserves the established app theme and capture warning.

Final local verification passed 62 frontend tests, zero Svelte errors/warnings, the production frontend build, and whitespace checks. No audio processing changed and no paid API test was repeated for this presentation patch. Release/installation results are recorded separately after they run.


### v0.4.1 release and installed artifact

[Release run 35286400748](https://github.com/dweng1572179/mac-meeting-notes/actions/runs/35286400748) passed all frontend tests/type checks, Rust tests, Clippy, the optimized macOS build, packaging regression, and mounted-app signature verification. [v0.4.1](https://github.com/dweng1572179/mac-meeting-notes/releases/tag/v0.4.1) points to `18f7cf25fffd65d47b50de2837fe42ac7add9251`.

The public DMG is 6,148,357 bytes with SHA-256 `c0ad31a96af20f3d482f8a41eb888ca8372fa0ec61866026f409e506c89ab9ef`. Its local digest matched GitHub, and the read-only mounted app passed strict deep signature, version, and audio-input entitlement checks. Installation followed a fresh inactive-recording check and graceful quit. Every installed bundle file matched the release; executable SHA-256 is `160fc3a4354b7d1256fae2d17c4c3e194813fdd12642e14a2f175019fce19c3a`.

Both existing meeting files remained byte-for-byte unchanged. Build caches, verification browser/server, synthetic harness, screenshots, mount, and rollback bundle were cleaned. One verified DMG remains in Downloads.

The installed window launched, but the native UI check is pending the user's macOS Keychain approval. A targeted process sample confirmed bootstrap waiting in `SecKeychainFindGenericPassword`; inspection timed out. Browser UI verification passed as recorded above, but native UI verification is not marked passed. No credentials or Keychain access settings were changed by the agent.


## v0.4.2 unified notes and grounded questions — 2026-09-17

The empty Your notes view was a separate editor for manual notes, while the useful summary lived in AI notes. There is now one Notes document and one autosave path. An optional canonical notes field leaves legacy fields intact. On read, older summaries retain manual text that is not already present verbatim; this is a lossless compatibility view, not a data rewrite. An intentional canonical blank stays blank. Transcript deletion retains the visible document.

Independent review found two migration/timing defects before release: legacy manual text could be hidden, and a queued metadata save could replay acknowledged note text over a newly generated result. Legacy merge checks and note-revision-aware autosave fix these paths. The reviewer verified both fixes with no remaining blocker. Generation also compares the requested document against the current saved document before replacing it.

The previous question contract allowed no-citation unavailable answers while its validator rejected all such responses. It also depended on the model copying exact excerpts. The replacement uses stable passage IDs, resolves source text locally from the same request snapshot, rejects unknown/uncited supported answers, deduplicates citations, and provides a fixed no-information answer when unsupported. Full source coverage remains bounded and never silently truncated. Exact passages establish source provenance; they do not prove every model inference correct.

Local checks passed the full Rust suite and focused lifecycle/store/API contracts, including Unicode preservation, late decisions, no-answer behavior, intentional blank notes, legacy manual additions, deletion retention, oversize rejection, and in-flight edits. Frontend checks cover the queued metadata race and unified export/search selection. Browser checks verified existing summaries, edit → navigate → reopen with exact Unicode text, explicit blank edits, and desktop/840px layouts without ownership tabs or horizontal overflow.

The opt-in `live_synthetic_meeting_questions_with_openai` acceptance passed two text-only requests in 3.6 seconds: “what was it about” returned a warehouse review summary with two exact saved passages; an unmentioned phone number produced the normal no-information answer. It used only synthetic text and the already-authorized login Keychain credential, never a saved user recording. No audio was recorded or retranscribed.

Final local verification passed 56 frontend/API tests, zero Svelte errors or warnings, the production frontend build, all-target Clippy with warnings denied, Rust formatting, and diff whitespace checks. The full Rust suite passed with paid tests excluded; the synthetic question check passed separately as recorded above. Release and installation results are recorded after artifact verification.


### v0.4.2 release and installed verification

[Release run 35291401937](https://github.com/dweng1572179/mac-meeting-notes/actions/runs/35291401937) passed the frontend checks, full Rust suite, Clippy, optimized app build, packaging regression, and mounted signature verification. [v0.4.2](https://github.com/dweng1572179/mac-meeting-notes/releases/tag/v0.4.2) points to `c7940bdb916fbe7d96a1dfd6df13a64b7c8df8e6`.

The public DMG is 6,167,780 bytes; SHA-256 `babaca1553d4f8bc2872f056a1e2f54e17bbb92a6a93b1ef9996967ae8d77030` matched GitHub. Its read-only mounted app passed version, strict deep signature, and audio-input entitlement checks. Both saved sessions were complete immediately before installation. The running app returned to the library to flush edits, quit normally, and was replaced only after the process exited and session states were rechecked. Every installed bundle file matched the public artifact; executable SHA-256 is `329a596f3c0bb9375d672e43d52bc0ae451c266b939c7156cd1388e617e56bdd`.

Native UI verification passed: the existing TEST meeting opened under Notes with Edit notes and Transcript controls, without the old ownership tabs. The exact reported question, “what was it about”, succeeded in the installed app and displayed nine locally resolved saved-source citations. This additional native check used TEST's saved text; it did not record audio, retranscribe either meeting, or open the protected lecture. Both existing meeting JSON files remained byte-for-byte unchanged after the question.

Verification browser/server, local build caches, temporary installer/mount, and rollback bundle were cleaned. The latest verified DMG remains in Downloads, and the installed app remains open on TEST with its successful answer.


## v0.4.3 speaker timestamp recovery — 2026-09-22

A retained recording failed with `invalid_transcription` after the provider returned unusable speaker timing. Repeating the affected upload returned valid text and timing, so the exact malformed value in the original discarded response could not be recovered. The deterministic regression reproduced the same saved error using a zero-duration turn.

The fix treats speaker annotations as optional when nonempty transcript text is available. Invalid annotations are discarded without manufacturing timestamps or requesting another transcription; the text and a speaker-label notice are saved together. Both provider-reported duration and actual saved chunk duration are checked, for recording-time sections and legacy files. Strict persisted-segment validation and save-before-audio-cleanup remain unchanged. Inconsistent empty responses, real HTTP failures, and output-limit detection still fail safely with audio retained.

The focused tests failed before implementation and passed afterward. Coverage includes zero/negative/out-of-range timestamps, missing labels/segments/duration, duplicate IDs, text preservation, durable notices, both processing paths, and no extra HTTP request. The ordinary Rust suite passed 145 checks (three paid tests excluded); all-target Clippy denied warnings, formatting and whitespace checks passed. Independent read-only review found no blocking issues. All 56 frontend/API tests passed; Svelte reported zero errors/warnings, and the production frontend build passed. Public-artifact, installation, and actual recovery results follow after their verification.


### v0.4.3 public artifact and installation

[Release run 35772251977](https://github.com/dweng1572179/mac-meeting-notes/actions/runs/35772251977) passed frontend/backend checks, Clippy, the optimized Apple-silicon app build, packaging regression, and the mounted signature/entitlement checks. [v0.4.3](https://github.com/dweng1572179/mac-meeting-notes/releases/tag/v0.4.3) points to `6aa36c70232f4403b9a933a4b580770c28a22725`.

The downloaded public DMG is 6,170,578 bytes; its SHA-256 `8f5511a3bf89dc6d12aaff60df3888710f5f03a2693bc0edf526047387c36e19` matched GitHub. Read-only mounted version, strict deep signature, and audio-input entitlement checks passed. Fresh session and native UI checks showed no active recording; the editor returned to the library before normal quit. Installation checked that the process had exited and sessions were inactive again before replacement. Every installed bundle file matched the public artifact; executable SHA-256 is `0cc3b0a8ca4a85c44895c4d51e58c7087e8230c9d119a78167404ec78b41eedf`.

The installed app loaded the library without a manual permission prompt, opened the failed meeting, and resumed from its eight saved checkpoints. Recovery completed as verified below. Installer staging, mount, rollback copy, local build caches, and completed diagnostic scripts were removed. One verified public DMG remains in Downloads.


Actual recovery in the installed app completed all 32 parts, retaining the eight original checkpoints exactly. It produced a nonempty transcript and Notes document. Two subsequent unusable speaker-annotation responses fell back to their returned text without stopping processing; the native notices panel showed the explanation. Unsupported optional AI suggestions were omitted without blocking the saved notes.

Native navigation back to the library and reopening verified the Notes sections, Edit notes control, complete Transcript view, Copy/Export controls, and absence of the incomplete-processing banner. The app was left open on the completed notes. Saved timing annotations still satisfied the strict validator; original meeting metadata and both older session files were unchanged. No private meeting text or source audio was copied into the repository. Verified recovery copies and the private diagnostic response were removed after completion, and no task-started watcher, build, server, or browser session remained running.
