# Onboarding and optional integrations

Decision guide, checked against official documentation on 23 September 2026. Provider connections and managed billing below are proposed work, not capabilities enabled by this document.

## Product model

Keep Meeting Notes accountless: users can open their library and type notes without providing an email address, connecting a calendar, or adding an API key. Offer **Set up AI** and **Continue with local notes** at first use; do not interrupt an existing library with forced onboarding. Recording with transcription and AI features currently require the user's OpenAI API key.

Use the existing bring-your-own-key model first. Explain that OpenAI API usage has separate billing from a ChatGPT subscription, with charges determined by API usage. Link to OpenAI billing and current prices rather than promise a fixed cost per meeting. [OpenAI billing](https://help.openai.com/en/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform), [API pricing](https://developers.openai.com/api/docs/pricing).

An optional managed plan can come later if users need AI without managing a key. That requires a real billing/identity backend, protected service credentials, usage enforcement, abuse controls, account deletion and revised data disclosures. Never ship a shared paid API key in the desktop installer. A managed plan should not become a prerequisite for accessing existing local notes.

## Describe storage and remote processing accurately

- **Local library:** sessions, notes, metadata and transcripts are ordinary JSON files; question history and drafts use webview local storage. These are not encrypted by the app. Device encryption, OS access controls and backups are separate considerations. See [session storage](../src-tauri/src/store.rs) and [question persistence](../src/lib/questions.ts).
- **Protected credential:** the API key uses the OS credential store, backed by macOS Keychain or Windows Credential Manager. Protecting the key does not encrypt the library. See [credential storage](../src-tauri/src/secrets.rs).
- **Remote AI:** audio and transcription context go to OpenAI for transcription; notes, transcripts, questions and relevant history go to OpenAI for enhancement/answers. The current native client calls `/audio/transcriptions` and `/responses`, with `store: false` for text requests. See [OpenAI client](../src-tauri/src/openai.rs). Typed notes without AI are local; the product is not fully offline AI.
- **Provider retention:** OpenAI says API data is not used for model training unless explicitly opted in. Its endpoint table lists no abuse-monitoring or application-state retention for `/audio/transcriptions`; text endpoints differ. Abuse-monitoring logs generally last up to 30 days, with legal/safety exceptions, and some features retain application state. `store: false` is not a blanket zero-retention guarantee. Zero Data Retention requires eligibility and approval and has endpoint/model limitations. [OpenAI data controls](https://developers.openai.com/api/docs/guides/your-data).

Explain retained recovery audio and deletion behavior where those actions occur. Removing a local credential does not revoke that key at OpenAI, cancel already submitted requests, remove remote retained data, or erase saved notes. Do not describe file deletion as secure disk erasure.

## Minimal onboarding and settings

First use needs one short choice screen, a focused AI setup step when selected, and a ready-to-use note. AI setup should explain remote processing and separate API charges before key entry. Show actionable validation/save failures without losing typed notes. Request recording permissions when starting recording, describe microphone plus computer-audio capture, and remind the user to obtain appropriate participant agreement. Do not claim permission or device checks passed unless actually checked.

| Settings section | Useful information and actions |
|---|---|
| AI | Key present/missing/unavailable state; save/replace/remove; fixed official links for keys, billing and pricing; model/cost settings. Never display a saved secret. |
| Recording | Capture behavior, permission help, language and vocabulary; explain what an audio issue affects and how to retry. |
| Privacy & data | Local storage versus remote processing, unencrypted library, retention/recovery behavior, export/deletion, provider data-policy links. |
| Connections | Optional calendar status, account identity and granted access; connect/reconnect/disconnect when implemented. Until then, show honest explanatory text without working-looking sign-in buttons. |

Keep cancellation harmless, keyboard focus predictable, failures recoverable, and setup usable at the supported minimum window. A credential-store failure must not lock the user out of local notes.

## Calendar-first provider connections

Use **Connect Google Calendar** or **Connect Microsoft Calendar**, not a mandatory Meeting Notes login. OAuth authorizes provider access; optional OpenID Connect identity identifies the connected account. Basic identity scopes do not grant calendar or mailbox access. The desktop app can call these providers directly without a Meeting Notes account server. [Google native OAuth](https://developers.google.com/identity/protocols/oauth2/native-app), [Microsoft desktop configuration](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-app-configuration).

| Provider | Start with the smallest permission supporting the chosen feature |
|---|---|
| Google | `calendar.events.owned.readonly` for owned calendars; `calendar.events.readonly` if shared calendars are needed. Add `calendar.calendarlist.readonly` only for a calendar picker. Avoid calendar write access. [Scope definitions](https://developers.google.com/workspace/calendar/api/auth). |
| Microsoft | Delegated `Calendars.ReadBasic` supports `calendarView` for personal and work/school accounts. It excludes event body, attachments and extensions. Use `Calendars.Read` only when richer content is needed; shared-calendar access needs its own scope review. [CalendarView permissions](https://learn.microsoft.com/en-us/graph/api/calendar-list-calendarview?view=graph-rest-1.0), [Graph permissions](https://learn.microsoft.com/en-us/graph/permissions-reference#calendarsreadbasic). |

Fetch a bounded upcoming-event window and let the user choose an event to associate with a note. Import only the title/time/participants needed. Calendar connection should not silently add every event to AI requests; disclose any selected calendar content subsequently sent for AI processing.

Use native authorization code flow with PKCE S256, fresh random state and the system browser. Bind a temporary callback listener only to loopback; validate the pending provider/state and close it after completion, cancellation or timeout. Google supports a random desktop loopback port; Microsoft documents a registered `http://localhost` system-browser redirect. Handle token exchange/refresh in native code, store tokens in OS credential storage, and keep them out of webview state/logs/exports. Do not rely on a bundled client secret. Check actual granted scopes and show reconnect/admin-required states without disabling local notes. Request Microsoft `offline_access` when persistent access is needed. [Google flow](https://developers.google.com/identity/protocols/oauth2/native-app), [Microsoft flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow), [token storage](https://developers.google.com/identity/protocols/oauth2/resources/best-practices).

Disconnect stops fetching and deletes local tokens/cache. Google supports revocation, but it invalidates all grants under the project, not just one calendar permission. Microsoft local token deletion is distinct from provider-consent removal; link to permission management and explain administrator-controlled grants. Google installed apps do not support web-style incremental authorization: adding a future feature needs a deliberate new authorization flow. [Google revocation and native constraints](https://developers.google.com/identity/protocols/oauth2/native-app), [Microsoft consent removal](https://learn.microsoft.com/en-us/entra/identity-platform/howto-update-permissions).

## Keep mailbox access separate

Gmail `gmail.readonly` **and** `gmail.metadata` are restricted scopes. Public mailbox import can require restricted-scope verification and a security assessment when restricted data is stored/transmitted through servers, subject to applicable exceptions. Sending imported email to a remote AI service is a relevant server data flow; desktop packaging alone does not exempt it. Do not add Gmail or Microsoft mail permissions to calendar onboarding. [Gmail scopes](https://developers.google.com/workspace/gmail/api/auth/scopes), [restricted-scope review](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification).

## Owner-supplied registration artifacts and launch gates

| Before implementation/release | Required owner input or evidence |
|---|---|
| Google registration | Owner-controlled Cloud project, enabled Calendar API, Desktop app client ID, declared scopes, app/support identity, consent-screen audience and test users. Public launch needs applicable scope/brand verification; the Console reports current scope categories. Supply public homepage/privacy URLs and domain verification plus the requested demonstration. Personal/internal/testing exceptions are separate from public distribution. [Consent setup](https://developers.google.com/workspace/guides/configure-oauth-consent), [verification](https://developers.google.com/identity/protocols/oauth2/production-readiness/sensitive-scope-verification). |
| Microsoft registration | Owner-controlled Entra app/client ID, Mobile and desktop redirect configuration, delegated scopes and intended audience. Choose multitenant plus personal accounts only if both Microsoft 365 and Outlook.com are intended. Tenant policy can require admin approval even for normally user-consentable permissions; public enterprise adoption may need publisher verification. [Account audiences](https://learn.microsoft.com/en-us/entra/identity-platform/supported-accounts-validation), [publisher verification](https://learn.microsoft.com/en-us/entra/identity-platform/publisher-verification-overview). |
| OAuth acceptance | Test cancellation, wrong/replayed state, denied scopes, browser return, expired/revoked tokens, rotation/storage failures and disconnect on macOS/Windows. Google External/Testing refresh tokens expire after seven days for calendar access; do not mistake this for a finished production connection. [Google token expiry](https://developers.google.com/identity/protocols/oauth2#expiration), [Microsoft refresh-token lifecycle](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens). |
| macOS distribution | Developer Program/team identity, Developer ID signing capability and secure notarization credentials. Sign with correct entitlements/hardened runtime, notarize, staple and verify a downloaded artifact. Local ad-hoc signing is not evidence of public distribution readiness. [Apple distribution guidance](https://developer.apple.com/developer-id/). |
| Windows distribution | A trusted signing identity/service appropriate to the installer, with private material supplied through secure build secrets. Verify the signed installer and clean install/upgrade. Signing does not guarantee immediate SmartScreen reputation. [Microsoft signing guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options). |
| Published privacy policy | A public policy matching actual storage, remote AI, calendar fields, sharing, retention, deletion and support contact. Include any later managed service/mailbox flow before enabling it. Google public OAuth review requires truthful consent branding and privacy disclosures. [Google policy requirements](https://developers.google.com/identity/protocols/oauth2/production-readiness/sensitive-scope-verification). |

Client IDs, audiences and approved URLs can be recorded as project configuration. API keys, signing private keys and token material belong in secure local/CI credential storage, never this guide or source control. None of these registrations, reviews or signing credentials is created by adding onboarding UI.
