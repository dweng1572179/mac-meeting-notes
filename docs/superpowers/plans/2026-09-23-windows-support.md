# Windows Support Implementation Plan

**Goal:** Publish a usable Windows installer without regressing Mac recording or saved data.
**Architecture:** Shared Tauri application and processing; platform capture backends and native secret stores; persisted audio format; portable PCM WAV utility.
**Spec:** ../specs/2026-09-23-windows-support.md
**Execution:** Autonomous implementation authorized by the user. Independent native capture, pipeline, and question UI tasks use parallel workers; parent owns WAV utility, AI follow-up contract, packaging, dependencies, integration, verification and release.

**Scope update:** User supplied a Granola style reference and explicitly requested question UI/AI improvements in parallel. Preserve the warm app identity; replace the crowded question sections with a shared restrained conversation, contextual follow-ups, formatted answers and clear recovery. The last four completed exchanges may resolve references but are never evidence. Verify source IDs through a closed response schema and current saved passages. A Mac update for this UI may be installed only after verifying capture is idle.

## Global constraints
- Existing isolated worktree only; preserve original meetings and leave the installed Mac app untouched.
- No FFmpeg/Electron/runtime daemon. One heavy local build at a time, at most two compiler jobs.
- Windows x64, Windows 11 primary; Mac arm64/macOS14.2 unchanged. Existing release remains available until replacement passes checks.
- No claims of physical Windows recording validation from compilation alone.

## Tasks and checks
- [ ] Portable WAV (`audio.rs`, `audio_wav.rs`): write failing read/write/slice and malformed-data tests; implement mono PCM16 using bounded I/O, exact lengths, create_new, <=24 MB chunks. Preserve native M4A dispatch. Run native-independent Rust tests on both OSes.
- [ ] Capture (`recorder.rs`, `recorder_windows.rs`): use the unchanged NativeRecording interface; WASAPI mic + endpoint loopback, PCM16/16k conversion, bounded packet queue, independent finalization, one source failure preserves the other, explicit missing/overflow warnings, idle-sleep guard, deterministic stop/drop. Test synthetic queued packets, rotation/failure handling; Windows CI compiles all native APIs.
- [ ] Pipeline/schema (`domain.rs`, `store.rs`, `commands.rs`, `live_processing.rs`, `openai.rs`, related tests): persist `AudioFormat` with M4A legacy default; exact per-format names, durable Windows replacement, PCM MIME/validation, restart/retry/deletion tests using real WAV. Never delete pending audio after failed checkpoint.
- [ ] Windows integration (`Cargo.toml`, lock, keychain test, frontend copy, platform Tauri config, workflows): native credential persistence, OS-appropriate shortcuts/help, NSIS/WebView2 prerequisites, both platform checks and a single release publish job. Installer smoke install/launch/exit and uninstall uses a disposable CI profile.
- [ ] Review and release: inspect all changes for data loss and FFI lifecycle issues; run Mac regressions plus Windows CI, verify downloadable artifacts, document first-install steps and hardware-test limits, merge and publish. No Mac app upgrade is required for this Windows request.

## Review focus
- A microphone/device disappears while the other track is healthy: preserve both files and clearly flag missing capture.
- Disk or checkpoint writes fail: existing notes and unfinished audio survive, no false completion.
- Restart after an unregistered/unfinished WAV tail: discover only canonical owned files and keep unreadable tails.
- Old M4A sessions and foreign/malformed paths: no schema rewrite, cross-session reads or cleanup.
- Windows credentials/runtime/installer: no mock keyring fallback, per-user setup, no automatic capture at launch.
