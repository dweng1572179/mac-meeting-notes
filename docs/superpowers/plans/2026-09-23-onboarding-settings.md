# Onboarding and Settings Implementation Plan

> **For agentic workers:** Use the existing parallel ownership assignments and focused verification. User authorization covers implementation; missing provider scope/audience choices remain optional asynchronous questions.

**Goal:** Make first use and settings clear and safe for a local desktop product, while improving note/chat access.

**Architecture:** Reuse native keyring, backend validation, current settings persistence and Svelte components. Keep first-run dismissal as non-sensitive local UI state. Keep tokens and all external URLs/paths behind a fixed native command boundary.

**Tech Stack:** Svelte 5, TypeScript, Tauri 2, Rust; existing Node/Vitest/Rust tests.

**Spec:** ../specs/2026-09-23-onboarding-settings-design.md

## Global constraints

No new dependencies; existing worktree only. Synthetic verification only, preserve user data. One expensive local check at a time.

## Review focus

- Credential store locked/unavailable: local sessions still load, AI error remains visible.
- Removal attempted during capture/processing or failed key replacement: preserve ongoing work and correct saved-key state.
- Unavailable WebView storage: onboarding can still be dismissed for the current run without blocking notes.
- Fresh empty library vs returning profile: setup is optional and never starts capture/network on mount.
- Minimum window, long settings text, keyboard-only navigation and long chat: useful controls remain reachable.

## Tasks

- [x] Backend: add nullable `keyAccessError` and `dataDirectory` to bootstrap; idempotent `remove_api_key`; fixed `open_settings_destination` allowlist; validate key input before network; add production CSP and separate dev policy. Add focused native tests, verify red then green without accessing the production keychain account.
- [x] Frontend: add tested onboarding visibility/dismissal helpers and native API wrappers; add inline skippable welcome; reorganize existing settings into AI, Recording, Privacy & data, Connections; include key removal confirmation and truthful permission/data information. Preserve draft preferences on section changes and report unsaved edits before dismissal.
- [x] Core UI: compact metadata and provide accessible meeting/library chat without burying its composer. Preserve autosave/recovery/citation behavior and existing visual system.
- [x] Guidance: save current primary-source OAuth/billing/privacy advice, required registration artifacts and release gates in `docs/onboarding-and-integrations.md`.
- [x] Verify: focused unit tests, Svelte check and production build; bounded browser test at standard/minimum sizes with synthetic data; disposable native startup/IPC/CSP checks; independent review and one batch of fixes.
- [ ] Record exact validation and remaining integration/parity scope. Commit reviewable changes; release/install only after applicable checks and idle safety verification.
