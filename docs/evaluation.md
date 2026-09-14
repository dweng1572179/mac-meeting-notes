# Focused evaluation

This check uses labeled synthetic content only. It tests that a later final decision overrides an earlier tentative suggestion, while the tester's sparse note and simulation labels remain intact.

## Live run

1. Build and open Meeting Notes on an Apple silicon Mac running macOS 14.2 or later.
2. In OpenAI settings, enter a valid API key with paid API access. Keep the key in the app; do not put it in the repository or shell history.
3. Start a new meeting and grant System Audio Recording permission when macOS asks.
4. Type only `rent roll is the issue` in the note editor.
5. While recording, play `fixtures/simulation-script.txt` through Mac system audio. The built-in `say -f fixtures/simulation-script.txt` command is one repeatable way to do this.
6. After playback ends, stop the meeting and wait for processing to complete.
7. Check both the Original and Enhanced views against the checklist below.
8. Quit Meeting Notes, reopen it, select the same meeting, and check both views again.
9. Confirm the completed meeting has no retained `.m4a` file in the app's `audio` data directory.

```text
[ ] Original note preserved verbatim
[ ] Rent-roll concern prominent
[ ] Tentative proceed suggestion not reported as final
[ ] Final pause decision present
[ ] Simulation labels retained
[ ] Raw audio deleted after transcription
[ ] Original and Enhanced views survive app restart
```

## Current status

The live OpenAI and hardware acceptance run above is unexecuted because this verification did not use a paid API key. No live capture or model result is claimed.

The reviewed Task 8 source tree has the following completed automated evidence:

- OpenAI request/response contracts: 5 tests passed, including the Responses endpoint, `store: false`, dynamic session content, and strict output parsing.
- Frontend/API behavior: 2 Node tests and 10 Vitest tests passed, covering the editor flow, request boundary, close/save ordering, and restart-facing state behavior.
- Recovery and storage: 9 lifecycle tests and 11 session-store tests passed, including interrupted recording/processing recovery, transcript-preserving retry, exact-path deletion, and deferred cleanup.
- Full Rust source suite: 37 tests passed; clippy passed with warnings denied.
- Packaging: one unsigned Apple silicon DMG built successfully before the verified `src-tauri/target` directory was removed for storage cleanup.

These automated checks do not substitute for the unchecked live OpenAI/system-audio run.
