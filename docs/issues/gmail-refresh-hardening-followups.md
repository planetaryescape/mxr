# Harden Gmail refresh diagnostics and timeout coverage

Status: all six follow-ups from the independent review of
`e5654fced9d0dabfd25485089b24cefe44faf606` are resolved on
`fix/gmail-refresh-hardening`, with one documented gap in item 3 (the
Retry-After hint is honoured by sync backoff, not by concurrent API calls).
The two diagnostic-polish notes that came later are resolved with them. The
original findings were source audits and proposed synthetic reproductions, not
evidence of a real account failure or credential leak.

The refresh-only background path is unchanged: missing or revoked grants fail
without starting browser or device authorization, and an unknown response is
never treated as proof of revocation. Credentials, response bodies and URLs
stay out of errors, logs and fixtures.

## A failed cache write after a good refresh no longer reads as sign-out

Resolved. `AuthError::TokenCacheWrite` ("Local token cache write failed: ...")
replaces the `OAuth2` wrapper in `crates/provider-gmail/src/auth_refresh.rs`.
`GmailClient::auth_header` maps it to `GmailError::Other`, so the message does
not carry the "Auth error" label. The daemon classifies it as `store_index`
(`crates/daemon/src/loops.rs`, sync health "sync failing" with the message
shown), and doctor reports a `Storage` finding that says the sign-in is intact
and points at the keychain and token directory. No finding prescribes
`mxr accounts add`. The stored grant is untouched by the failure.

## A malformed token-endpoint response keeps its HTTP status

Resolved. `read_refreshed_token` in `auth_refresh.rs` is the single reader for
the background helper and the older explicit helper (`with_refresh_token`):

- A non-JSON or unreadable 4xx, a JSON 4xx with no OAuth error code, and a
  malformed 200 become `RefreshUnavailable` with the status ("returned an
  unreadable HTTP 400 response"). Doctor and sync health treat that as
  retryable.
- Only an explicit allowlist of RFC 6749 section 5.2 rejection codes becomes
  `OAuth2` and prescribes action: `invalid_grant` (reauthorize), plus
  `invalid_client`, `unauthorized_client` and `invalid_scope`. For the last
  three doctor first names the real cause (check the OAuth client, or enable
  the Gmail API and list the scopes), since re-adding the account cannot fix
  them, and keeps reauthorization as the final step. Every other code, known
  (`invalid_request`, `temporarily_unavailable`, `server_error`) or not, stays
  retryable with the code and HTTP status in the message. Text in the `error`
  field that is not shaped like a code is never copied into diagnostics.
- Transport failures are reduced to a fixed category (timed out, connection
  failed, TLS handshake failed, response could not be read), so reqwest URLs,
  hosts and ports never appear. A timeout while reading the body is reported as
  a timeout: reqwest 0.13 flags it as both timeout and decode, so the timeout
  is checked first.
- Redirects are reported with their status, not followed.

The older helper no longer propagates `error_description` (diagnostic polish,
second note). Local-endpoint tests cover each case, including that no response
body marker reaches an error string.

## A token-endpoint 429 carries its Retry-After to the sync backoff

Resolved for sync, with one gap. `AuthError::RefreshRateLimited` carries the
`Retry-After` seconds (default 60 when absent or an HTTP date, capped at
3600). `GmailClient::auth_header` turns it into `GmailError::RateLimited`,
which becomes `MxrError::RateLimited`, so `next_backoff_secs` waits the hint
plus a margin (30 to 300 seconds) and sync health shows "paused: rate
limited". It is reported before any mail request is built, and sends are not
retried by the daemon (an uncertain send is never resent), so a token throttle
cannot duplicate a message. A test pins that three client calls make three
token requests and no mail request.

Remaining gap: concurrent API calls inside one sync pass each still ask the
token endpoint once while it is throttling, because the source does not
remember the throttle between calls. The pass fails on the first error and the
daemon backs off, so the cost is a few extra 429s. A `throttled_until` on the
cached source would remove them if that ever shows up in logs.

## Network-before-auth keyword precedence is pinned across providers

Resolved. `provider_failures_classify_the_same_in_sync_health_and_doctor` in
`crates/daemon/src/handler/status_helpers.rs` runs 22 synthetic Gmail, IMAP and
SMTP errors through sync health and doctor: rejected credentials, connection
reset and TLS failure inside the IMAP login wrapper, XOAUTH2 token-fetch
failures (transport and `invalid_grant`), SMTP credential rejection, and every
Gmail refresh cause above. The precedence is correct for them. The table
exposed one real mismatch: doctor reported a Gmail 401 or an IMAP password
rejection as a plain sync error with no advice while sync health said "sign in
again". Doctor now agrees with `classify_sync_error` for auth-class failures
and gives the reauthorization step.

Known limit, not changed: a server's rejection text that itself contains a
network word (a Dovecot `PRIVACYREQUIRED` message mentioning SSL/TLS
connections) classifies as network. Narrowing the keywords further needs
structured provider errors, not more string matching.

## The outer timeout does not preempt a blocked cache write, and that is acceptable

Measured, boundary unchanged. With a storage whose `set` blocks its thread for
600 ms and a 10 ms heartbeat task:

| runtime | heartbeat max gap | 100 ms guard around `access_token()` |
| --- | --- | --- |
| current-thread | 610 ms (whole runtime stalled) | returned the token after 602 ms |
| two-worker multi-thread (the daemon's flavor) | 13 ms | returned the token after 602 ms |

On the daemon's runtime a stuck write pins one worker and leaves the rest
running, and the guard is not enforced for that step. The macOS keychain path
disables user interaction, so it returns promptly; no hang has been observed.
Moving the write to `spawn_blocking` would restore the guard but has to keep
serialization and token rotation correct through the final result (a timed-out
caller must not let an older write land after a newer token), and nothing
measured justifies that risk. The `TOKEN_REFRESH_TIMEOUT` comment now states
the limit, and
`a_blocked_cache_write_leaves_the_rest_of_the_runtime_running` pins the part
that holds. Revisit if a Linux Secret Service hang is ever reported; the design
constraint above is the starting point.

## Explicit authorization hands off to the refresh-only source

Resolved. `GmailAuth::authorize_with_storage` is the shared tail of both
explicit flows, taking the storage and secret as parameters; production passes
the keychain storage as before. Tests complete the installed flow (a delegate
plays the browser against the local redirect) and the device flow (local
device-code endpoint), count prompts, then acquire the cached token and
refresh it with no second prompt, for an omitted and a rotated refresh token.
Synthetic tokens and a wiremock endpoint only.

## Doctor reports one failure once

Resolved (diagnostic polish, first note). Doctor drops a log line that carries
the exact error already shown from sync status, and drops repeated identical
log findings, so one transport failure appears once at Error severity. A
different failure in the log still appears.

## Open: doctor's log dedupe ignores which account a line belongs to

`crates/daemon/src/handler/status_helpers.rs` drops a mirrored log line when it
contains any account's current sync-status error. If two accounts hit the same
failure text, account A's status finding can hide account B's log line even
when B has no status finding of its own. Match the account and the exact
sanitized error rather than a substring. Found by independent review of
`916bd0188`; not a release blocker.

## Verification for a follow-up change

Run focused tests through the shared build queue, reusing the worktree's
`target-cli` and worker limits:

```sh
agent-work run -- scripts/cargo-test -p mxr-provider-gmail --tests
agent-work run -- scripts/cargo-test -p mxr --lib -- classify_sync_error classify_log_line oauth_transport_failures handler::freshness::tests::
agent-work run -- scripts/pre-pr-rust-gate
agent-work run -- cargo build -p mxr
```
