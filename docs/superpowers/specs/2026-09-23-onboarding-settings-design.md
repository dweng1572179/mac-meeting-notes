# Local onboarding, secure settings, and everyday UI

The user authorized work toward Granola competitiveness, with particular attention to local onboarding, settings, and secure Google/Microsoft integration. This first implementation keeps the existing desktop architecture and local data compatible.

## Product decisions

- Local notes need no Meeting Notes account, email address, provider connection, or API key. First use offers AI setup or immediate typed notes. Existing libraries never get a forced welcome flow.
- Recording and AI use the user's OpenAI API key. Explain that processing sends audio/text to OpenAI, that API billing is separate from ChatGPT subscriptions, and that saved keys are protected by macOS Keychain/Windows Credential Manager. Never call notes encrypted merely because the credential is secured.
- Settings have distinct AI, Recording, Privacy & data, and Connections sections. Show accurate states, recoverable failures, and native permission instructions without pretending permissions have been checked.
- Key removal is explicit and idempotent, preserves notes, is blocked during active recording/processing, and does not claim provider revocation or cancellation of in-flight questions.
- Credential-store failure must not prevent opening a healthy local library. Production web content uses a local-only CSP and narrowly scoped native commands.
- Google/Microsoft are optional calendar connections, subject to the user's pending scope/audience clarification and provider registration. No inert sign-in buttons, password collection, bundled OAuth secret, mailbox ingestion, or invented connected state. The integration/security guide records the required implementation and provider launch gates.
- Preserve the existing parchment/olive/serif visual language. Compact metadata, keep notes readable, and make chat/composer accessible at the existing 900×620 minimum window.

## Constraints

Use the existing worktree and Svelte/Tauri/Rust stack. No new dependencies. Preserve all user notes, credentials, recording recovery and data deletion semantics. Use only synthetic content for verification. Do not replace or interrupt the installed app during capture/processing. Serialize expensive checks on this 8 GB Mac.

## Acceptance

Fresh profiles can skip setup and create a note without network/capture; returning profiles keep their normal library. Settings expose honest storage/AI/permission information and working fixed-destination help controls. Failed key save/removal preserves the prior state; a credential lookup failure leaves notes available. Keyboard focus/escape/dialog scroll and the minimum window work. Native production CSP startup/IPC is checked on a disposable profile before claiming native verification.

## Remaining parity program

After this foundation: same-note recording resume; safe trash/restore; editable formatted notes and richer Markdown; named conversation history and streaming; retrieval beyond the most recent 20 meetings; configurable note templates; calendar-event association. These are separate behavioral changes, not claims made by this batch. Shared workspaces/cloud accounts and mailbox import require a concrete cloud/privacy/business model before implementation.
