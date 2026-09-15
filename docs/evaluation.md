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
[ ] Meeting audio and You transcript labels contain speech
[ ] Folder question links to an exact meeting excerpt
[ ] Both raw audio files deleted after transcription
[ ] Original and Enhanced views survive app restart
```

## Current status

The complete live OpenAI and hardware acceptance run above remains a manual release check because it requires speaking into the selected physical microphone. Automated and synthetic native checks cover the same storage and request boundaries without collecting private meeting content.

The source tree has the following automated evidence:

- OpenAI request/response contracts cover transcription, enrichment, `store: false`, source-bounded meeting questions, and rejection of invented citations.
- Frontend/API checks cover autosave, close/save ordering, exact folder filtering, the meeting-question boundary, and restart-facing state.
- Recovery and storage checks cover interrupted recording/processing, transcript-preserving retry, dual-audio deletion, exact-path validation, and deferred cleanup.
- Native recorder checks cover single-owner recording, safe callback shutdown, microphone error mapping, and preservation of both audio paths for retry.

These checks do not substitute for verifying the selected physical microphone before an important meeting.
