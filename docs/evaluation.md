# Focused evaluation

This check uses labeled synthetic content only. It tests that a later final decision overrides an earlier tentative suggestion, while the tester's sparse note and simulation labels remain intact.

## Live run

1. Build and open Meeting Notes on an Apple silicon Mac running macOS 14.2 or later.
2. In OpenAI settings, enter a valid API key with paid API access. Keep the key in the app; do not put it in the repository or shell history.
3. Start a new meeting and grant Microphone and System Audio Recording permission when macOS asks.
4. Type only `rent roll is the issue` in the note editor.
5. While recording, play `fixtures/simulation-script.txt` through Mac system audio. The built-in `say -f fixtures/simulation-script.txt` command is one repeatable way to do this. Also say `[SIMULATION] microphone check from the user` into the microphone.
6. After playback ends, stop the meeting and wait for processing to complete.
7. Check both the Original and Enhanced views against the checklist below.
8. Quit Meeting Notes, reopen it, select the same meeting, and check both views again.
9. Set its folder to `Acquisitions`, return to that folder, and ask `What was the final decision and what remains open?`. Confirm the answer cites this meeting and the citation opens it.
10. In Finder, choose **Go → Go to Folder**, enter `~/Library/Application Support/com.dweng.meetingnotes/audio/`, and confirm the completed meeting has no retained `.m4a` file there.

```text
[ ] Original note preserved verbatim
[ ] Rent-roll concern prominent
[ ] Tentative proceed suggestion not reported as final
[ ] Final pause decision present
[ ] Simulation labels retained
[ ] System audio and Microphone transcript labels contain speech with overlapping source offsets
[ ] Folder question links to an exact meeting excerpt
[ ] Both raw audio files deleted after transcription
[ ] Original and Enhanced views survive app restart
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
