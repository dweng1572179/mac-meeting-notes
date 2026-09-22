# Reliability audit — 22 September 2026

The repeated incidents came from several independent weaknesses. They were not evidence that transcription must wait until the end, or that a substantially more expensive model is required. The app already finalizes and uploads approximately one-minute source sections while recording; it writes final notes after stopping. A stopped uploader can leave a large backlog even though capture continues.

## Incident history and testing gap

The earlier long-recording incident involved a whole-file API rejection, missing capture coverage, and replacement of an ad-hoc-signed application during capture. Safe chunking, durable checkpoints, capture-health reporting and the prohibition on installation during recording addressed those mechanisms.

The later incident stopped after 8 of 32 sections because optional speaker timestamps were treated as fatal. Version 0.4.3 recovered the remaining sections by retaining usable text without invalid annotations. Capture-duration evidence for that incident was healthy. The recovered meeting remained complete at the start of this audit; the user clarified that the request concerned repeated failures and overall reliability, not another newly failed recording.

Earlier tests established decoding, persistence and a narrow single-voice success path. They did not establish resilience to every response-body failure or representative bilingual notes quality. Passing those tests was insufficient justification for describing the overall product as finished.

## Findings and changes in 0.4.4

| Layer | Reproduced weakness | Change and regression coverage |
| --- | --- | --- |
| API parsing | Null, missing or wrongly typed optional speaker fields rejected otherwise usable words before timestamp fallback ran. | Required text remains strict; optional annotations are parsed separately and discarded when invalid. Tests cover null fields, invalid types, missing fields and duration metadata. |
| HTTP transport | A successful HTTP status followed by a stalled/truncated body was labeled invalid model content. | Read the complete body before JSON decoding; retain timeout/network classification. Actual local HTTP tests exercise both cases. |
| Live uploader | Every error permanently paused uploading until Stop and Retry. | Network, timeout, rate-limit and server failures get at most three retries after 2/4/8 seconds, under the same processing lease. Auth/quota failures do not loop. Stop during backoff remains recoverable. |
| Isolated bad audio | A repeatedly rejected section prevented later sections from progressing. | Persist the section error and keep processing other and newly recorded sections. Stop still reports incompleteness; explicit Retry clears the failure marker and only missing sections upload. Original failed audio remains. |
| Transcript presentation | Timed annotations could omit words present in canonical text. | Whitespace-normalized disagreement falls back to complete canonical text without invented speaker attribution. Display, search and enrichment share this policy. |
| Capture health | Fresh callbacks could hide an accumulated recording gap. | After startup, more than five seconds of missing coverage remains visible even when current activity resumes. This warns about missing data; it cannot reconstruct it. |
| Notes editor | A clean focused editor could keep an old draft and overwrite newly generated notes on the next edit. | Clean drafts synchronize after generation; dirty drafts remain intact. A mounted Svelte check covers focus, completion, concurrent edits and the saved payload. |
| Status continuity | Refresh announced success before generation; parked failures looked like ongoing uploads. | Refresh announces starting. Parked sections say they need retry. |
| Summary quality | Generic business framing invented a Decisions entry for a conversation containing no decision. | Require substantive choices and explicit commitments, retain concrete details/negations/unanswered questions, and omit empty sections. A paid bilingual fixture reproduced the old behavior and passed with the revised prompt. |
| Evidence | Unchanged generated notes could become independent evidence on refresh. | Use original typed notes plus transcript sources when the visible document is the unchanged generated baseline. Request construction and suggestion validation use exactly the same source preparation. Human-revised documents are preserved whole. |

Durable storage still writes and syncs a temporary file, renames it atomically and syncs the directory. Audio cleanup follows durable transcript persistence. A crash after a paid response but before its checkpoint can require that request again; exactly-once billing is not claimed.

## Real API checks

Only disposable synthetic data was sent. No saved user meeting was retranscribed or summarized by these tests.

- **Spanish/English, two synthetic voices:** 44.3 seconds, two sections. The original prompt falsely classified an absence of future plans as a decision. With the revised prompt, notes retained specific food names, ingredients, quantities, family context, negation and an unanswered question; Decisions and Action Items were absent. The full pipeline completed, reopened its saved notes and cleaned checkpointed audio.
- **Final decision across a section boundary:** 105.4 seconds, two sections, 38 speaker turns. Verified a checkpoint before Stop, the final Monday decision replacing an earlier Friday proposal, the verification owner, grounded suggestions, saved notes on reopen and cleanup.
- **Mounted editor:** generation while focused, preservation of a dirty draft, the next saved payload, and truthful refresh-start messaging all passed using mocked native IPC.

These are small acceptance fixtures, not a word-error-rate benchmark, accent benchmark, or proof of reliable physical device switching. Model output remains probabilistic. A source quote establishes provenance, not semantic correctness.

Repeat the paid checks explicitly using `cargo test --manifest-path src-tauri/Cargo.toml --lib live_bilingual_conversation_quality_with_openai -- --ignored --nocapture --test-threads=1` and `live_synthetic_segmented_diarization_with_openai` with the same options. They use the authorized login Keychain credential and dispose of their synthetic libraries. Run the mounted check with `npm run dev`, then `agent-browser eval "import('/scripts/check-editor.js').then(m => m.checkEditor())"`; close the browser and dev server afterward.

## Cost and latency

Published rates checked on 22 September 2026:

| Path | One audio minute | One meeting hour with both tracks |
| --- | ---: | ---: |
| Current `gpt-4o-transcribe-diarize` | about $0.006 | about $0.72 |
| `gpt-transcribe` file transcription, not adopted by this release | $0.0045 | $0.54 |
| `gpt-live-transcribe` word-by-word streaming, not adopted by this release | $0.017 | $2.04 |

The app's `gpt-4.1-mini` summarizer is $0.40 per million input tokens and $1.60 per million output tokens. An illustrative 20,000-input/2,000-output-token summary costs about $0.0112. These are arithmetic estimates before retries and extra questions/refreshes, not measurements of an account bill. Both microphone and system tracks count, even when speech overlaps. Request usage is not currently persisted, so an exact historical bill cannot be reconstructed from the meeting file. See [OpenAI pricing](https://developers.openai.com/api/docs/pricing).

Minute sections reduce end-of-meeting backlog without a permanent realtime connection. Stopping still waits for the current/final section, any unfinished uploads and final summarization. Word-by-word streaming changes latency; it does not repair capture gaps, incorrect speaker metadata, editor races or invented summaries. See [OpenAI transcription guidance](https://developers.openai.com/api/docs/guides/transcription).

## Remaining product limits

1. Test real microphone/output combinations, sleep/wake, multi-hour capture, permission changes and AirPods reconnects. Microphone capture currently selects its device/format at startup; automatic rebinding is not implemented.
2. Preserve native audio timestamps through gaps before claiming exact alignment across sources. Current offsets follow each source's accepted audio and can compress gaps independently.
3. Evaluate overlapping voices and microphone bleed. The two tracks can contain the same remote speech; no cross-track echo removal or verified speaker identity matching is implemented. Speaker labels are scoped to each upload.
4. Grow a consented, representative speech/notes evaluation set including accents, code-switching, quiet speech, overlapping turns and domain vocabulary. Synthetic voices cannot establish this quality bar.
5. Developer ID signing/notarization is still needed for stable public distribution identity. Ad-hoc signing remains a beta limitation.

Version 0.4.4 addresses the reproduced software failures above. It does not make the product perfect or establish those remaining hardware and recognition guarantees.
