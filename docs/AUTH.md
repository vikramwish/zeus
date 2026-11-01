# Native ChatGPT subscription authentication

`src/auth.rs` implements the documented **public-client dynamic registration**
flow, without a client secret, partner key, another application's client ID,
Codex/Hermes credentials, or an undocumented ChatGPT backend. No real account
was connected during development; all token-bearing tests use local fixtures.

## Integration API

```rust
AuthStore::new() -> Result<AuthStore, AuthError>
AuthStore::at(path: impl AsRef<Path>) -> Result<AuthStore, AuthError>
login(&store, LoginMode::Continue, present) -> Result<AuthStatus, AuthError>
// present: FnOnce(&str) -> Result<(), AuthError>
status(&store) -> Result<AuthStatus, AuthError>
logout(&store) -> Result<(), AuthError>
access_token(&store) -> Result<AccessToken, AuthError>
list_models(&store) -> Result<Vec<SubscriptionModel>, AuthError>
```

- `new` obtains the account home from the OS user database, not job input or
  `HOME`, and uses `~/.config/zeus/auth.json`. `at` is an explicit trusted-caller
  override for tests/administration, **never a job configuration option**.
- `login` binds an ephemeral IPv4 loopback port **before** calling `present`.
  The trusted CLI should open the provided URL in the system browser (label:
  **Continue with ChatGPT**) and promptly return. Do not persist/log the URL.
  The callback is always `http://127.0.0.1:<port>/auth/callback`.
- `LoginMode::Continue` reuses the issued client ID and pins the verified
  subject. If no registration exists it starts dynamic registration.
  `NewAccount` is an explicit replacement of the **single saved registration**;
  it does not switch automatically or provide a multi-account picker.
  Either mode replaces existing credentials only after complete validation.
- `AuthStatus` exposes only `signed_in`, `needs_login`, `expires_at`, and a
  hashed `account_label`. This is local session state, not a remote validity
  check: an expired access token can still have renewable credentials.
- `AccessToken::as_str()` is the explicit secret boundary. Its `Debug` output
  is redacted; it is not `Display`/`Serialize`. Never put the returned bearer
  in jobs, worker environments, diagnostics, or evidence artifacts.
- `list_models` authenticates with that subscription token, accepts the
  subscription `models[]` schema, filters `visibility == "list"`, and returns
  `{id: slug, display_name}` in server order. There are no built-in model IDs.
- `logout` is **local only**: it clears access, refresh, and retained ID tokens,
  including the nonce, but keeps the opaque host UUID and client/subject
  mapping for later reauthorization. **Remote revocation is not confirmed**;
  tell the user to disconnect Zeus in ChatGPT Settings if desired. A caller
  already holding a bearer cannot be recalled by local logout.

Inference integration must pin the subscription route to
`https://api.openai.com/v1/responses`, use `store:false` and `stream:true`, and
require the terminal `response.completed` event. Never fall back to a paid API
key when subscription authorization, availability, or usage limits fail.
If the runtime retains a token for a whole task instead of requesting it again,
that task does not get mid-task refresh; surface expiry explicitly rather than
switching account or billing mode.

## Protocol and security invariants

The production endpoints are fixed:

| Purpose | Endpoint |
|---|---|
| Authorize | `https://auth.openai.com/api/accounts/authorize` |
| Token/refresh | `https://auth.openai.com/api/accounts/oauth/token` |
| JWKS | `https://auth.openai.com/.well-known/jwks.json` |
| Model catalog | `https://api.openai.com/v1/models` |

Initial requests use `client_id=dynamic_agent_client`, `agent_name_hint=Zeus`,
and a persisted random UUIDv4 host URN. Exchanges use only the issued client ID.
Returning requests omit `agent_name_hint`. Optional email/ID-token URL hints
are intentionally omitted; the browser may show the account selector.

Each attempt has fresh 256-bit state, nonce, and S256 PKCE verifier. Callback
state, method, path, Host, duplicate parameters, errors, code, and client binding
are checked before token exchange. The listener has a five-minute deadline,
16 KiB header limit, and bounded individual reads/writes. Its response is
constant and never reflects callback values. An invalid local request ends the
attempt safely; this is not protection against local denial of service.

ID tokens accept only RS256 and an unambiguous matching key from the fixed
OpenAI JWKS. Verification uses `ring` RSA verification, then checks issuer,
audience/authorized party, expiry, issue/not-before times, nonce, subject, and
`at_hash` if present. No token-supplied URL controls key retrieval. Returning
identity must match the saved subject. The token response must grant
`chatgpt.tokens.use.direct` and `resource.invoke`; renewable credentials also
require `offline_access`. Missing subscription permission never enables inference.
Access-token contents are otherwise opaque, not decoded as proof of identity.

HTTP requests disable redirects and inherited environment proxies, have
connection/read/write/overall timeouts, and cap JSON bodies at 1 MiB.
Service/library errors are replaced by fixed redacted messages. Endpoint
injection and shortened callback deadlines exist only under `cfg(test)`.

## Explicit filesystem fallback and rotation

This version uses the documented **private-file fallback**, not macOS Keychain.
It reuses the existing Unix/`libc` stack and adds no platform credential service.
It is plaintext protected by filesystem permissions, not encrypted storage.
The user account, same-UID processes, administrators, and backups remain within
its trust boundary. Full-disk encryption is recommended separately.

The credential directory must be owned by the effective user and exactly
`0700`. Credentials and the persistent lock file must be owner-owned regular,
single-link files with `0600` permissions. Directory traversal and opens use
`openat` with `O_NOFOLLOW`; absolute paths with symlinks or parent traversal
are rejected. Directory descriptors anchor subsequent operations. Writes use
exclusive `0600` staging files, file sync, same-directory atomic rename, and
directory sync. Staging names end in `.key` and the final name is `auth.json`,
matching existing workspace secret exclusions even after an interrupted write.
Do not place this directory in source control or widen its permissions.

A bounded `flock` serializes login, logout, status, and refresh across callers
and processes. Access-token retrieval refreshes near expiry (60 seconds),
respecting `earliest_refresh_at`. The request omits scope and sends the saved
issued client ID, refresh token, and fixed resource. Successful replacement
persists access/refresh/ID tokens, scope, and expiry as one transaction.

Before sending a refresh token, Zeus durably marks the session
`refresh_pending`. A lost response, rejected replacement, process crash, or
failed replacement write therefore cannot silently replay a possibly consumed
refresh token after restart. Such failures require explicit login; the module
does not automatically retry rotating refresh grants. A successful replacement
clears the marker. Optional omitted refresh scope/ID-token fields retain the
existing grant/ID token; any newly returned ID token is verified and identity
pinned. Login failures leave an existing validated session unchanged.

## Verification and limits

Run the focused test suite:

```sh
cargo test --test auth_flow --offline
cargo clippy --test auth_flow --offline -- -D warnings
```

The local HTTP authorization/token/JWKS/catalog fixture uses a generated,
non-production RSA key. Tests exercise PKCE and callback binding, dynamic and
returning registration, signature/issuer/audience/nonce/expiry/scope failures,
identity pinning, preservation on failed reauthorization, logout, bounded
callback waiting, filesystem permissions/symlinks/hardlinks, locking, concurrent
refresh, refresh-token rotation, remote errors, lost responses, failed rotation
persistence followed by restart, model filtering, rejected schemas, redaction,
response size bounds, and disabled redirects. No real login, live model catalog,
subscription inference, billing, revocation, or production token refresh was
performed. Real account eligibility and endpoint behavior require a later
user-completed login. Power-loss durability and hostile same-UID processes are
not proven by these tests.

## Official sources checked November 1, 2026

- [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)
- [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)
- [Token reference](https://developers.openai.com/siwc/token-sharing-open-source/token-reference)
- [OpenID discovery](https://auth.openai.com/.well-known/openid-configuration)
  was fetched directly without credentials to confirm issuer, endpoints, JWKS
  location, and RS256 support. The four documentation pages were retrieved
  directly from their official primary-source URLs.
