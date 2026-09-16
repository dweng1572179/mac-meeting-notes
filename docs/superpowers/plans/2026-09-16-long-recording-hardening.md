# Long recording hardening

Approved scope: autonomous v0.2.6 reliability release, existing isolated worktree only.

- [x] Reproduce 400 JSON error/request-ID loss with a local HTTP contract test. Parse safe error categories across all OpenAI callers; distinguish network and timeout failures.
- [x] Decode post-stop audio with native ExtendedAudioFile APIs and write independently finalized five-minute mono AAC chunks, bounded below 25 MB. Test real synthetic audio through the codec and independently decode every output.
- [x] Persist per-source chunk metadata and transcripts in existing atomic session JSON. Checkpoint before deleting audio; retry skips persisted chunks; preserve notes with the transaction lock. Exercise middle-chunk failure, reopen/retry, silent/empty sources, and storage failure.
- [x] Add atomic frame counts and callback times to each capture source. Poll from the UI, warn on missing/stalled/partial capture and changed executable identity, and retain final coverage warnings.
- [x] Run Rust/frontend suites, formatting, clippy, compilation and production DMG. Review security, retention and concurrency. Update version, README and roadmap.
- [ ] Commit, push and safely merge; publish release and verified DMG. Recheck recording state, install once into Applications preserving local data, reopen and verify. Remove generated build/test data and stop owned processes.

Design: no file rotation/disposal in real-time callbacks; bounded native decoding after stop. Chunk indexes are deterministic and source-specific. Overlapping source times are explicitly labeled as recording offsets rather than stitched into a fictitious conversation. Capture health is advisory: frame coverage is distinct from speech or wall-clock duration. Ad-hoc signing still cannot prevent macOS privacy identity resets.

Verification: 96 Rust checks, 25 frontend checks, separate live OpenAI acceptance, native APFS DMG signature/audio-entitlement verification, and atomic package publication under simulated disk-full failure.
