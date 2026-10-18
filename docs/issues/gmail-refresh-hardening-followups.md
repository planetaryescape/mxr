# Harden Gmail refresh diagnostics and timeout coverage

Status: nonblocking follow-ups from independent review of
`e5654fced9d0dabfd25485089b24cefe44faf606` (tree
`e240ef671f5786a0578a6813bfcc7a536f08447b`). The eight changed source files
match the original Gmail fix at `900d41d82d554cb302126fb7394ecc0156fcf98b`.
The review found no release blocker. These observations are source-audit
findings and proposed synthetic reproductions, not evidence of a real account
failure or credential leak. Local verification completion is tracked separately.

Preserve the refresh-only background path while addressing these cases. Missing
or revoked grants must fail without starting browser/device authorization;
unknown responses must not be treated as proven revocation. Keep credentials,
response bodies, and sensitive URLs out of errors, logs, and fixtures.

## Separate cache-write failure from rejected authorization

`crates/provider-gmail/src/auth_refresh.rs:78-81` wraps a failed `storage.set`
in `AuthError::OAuth2`. The concrete storage updates its in-memory cache before
persisting (`crates/provider-gmail/src/auth_storage.rs:290-302`). A local write
failure can therefore produce sign-out guidance even though the new token is
already available in memory.

Inject a synthetic persistence failure after successful refresh. Verify that
sync health and doctor identify a local storage problem, preserve the grant,
and do not prescribe reauthorization. Choose the owning error classification
rather than relabeling every cache failure as a network outage.

## Retain HTTP status when an error response is malformed

`crates/provider-gmail/src/auth_refresh.rs:98-108` parses JSON before handling
non-success statuses other than 429/5xx. A non-JSON 4xx becomes
`RefreshUnavailable`, so diagnostics lose the rejection's status. Conversely,
the status alone does not establish that the grant was revoked.

Add local endpoint cases for a non-JSON 4xx, a JSON `invalid_grant`, and a
malformed success response. Keep the status in sanitized diagnostics without
logging the body or automatically opening an authorization flow. Preserve useful
transport categories without exposing reqwest URLs or credential material.

## Preserve rate-limit scheduling evidence

`crates/provider-gmail/src/auth_refresh.rs:99-100` and
`crates/provider-gmail/src/auth.rs:473-474` classify token-endpoint 429 as
temporarily unavailable without carrying `Retry-After`. The daemon uses generic
backoff rather than the endpoint's scheduling hint.

Test a local 429 response with `Retry-After`. Decide how the provider can pass a
bounded retry delay through the existing error surface without confusing a
refresh throttle with expired authorization or retrying a mail send.

## Check network-before-auth classification across providers

`crates/daemon/src/loops.rs:1290-1304` now checks network words before auth
words. This fixes wrapped OAuth transport failures but also affects IMAP/SMTP
errors. `crates/provider-imap/src/session.rs` wraps both token-fetch and login
errors as authentication errors; their transport phase matters more than the
wrapper alone.

Add a table of synthetic provider errors covering rejected credentials, TLS or
connection failure during login, and OAuth `invalid_grant`. Confirm the intended
classification in sync health and doctor before narrowing or extending keyword
precedence. No additional cross-provider misclassification was reproduced in
the Gmail review.

## Qualify the outer timeout's synchronous-storage boundary

The 60-second guard at `crates/provider-gmail/src/auth.rs:663-670` covers async
acquisition, including waiting for the refresh mutex. The HTTP client bounds
connect to 10 seconds and the complete response to 30 seconds
(`crates/provider-gmail/src/auth_refresh.rs:36-41`). Those HTTP bounds do not
make synchronous OS calls preemptible.

`KeychainTokenStorage::set` calls synchronous keychain and disk persistence
(`crates/provider-gmail/src/auth_storage.rs:100-115,271-277`). The outer guard
therefore assumes those operations return promptly; it cannot interrupt a
blocked OS call on a Tokio worker. Before changing the execution boundary, use a
controlled blocking storage fixture to measure runtime responsiveness. Any
offloaded write must preserve grant rotation and hold serialization through the
final persistence result; timing out a caller must not permit a stale write to
overtake a newer token.

## Exercise the explicit-auth-to-background handoff

Installed and device flows pass a cloned `KeychainTokenStorage` to yup-oauth2,
then install the refresh-only source using the same shared cache
(`crates/provider-gmail/src/auth.rs:568-623` and
`crates/provider-gmail/src/auth_storage.rs:74-96`). Existing regression tests
exercise the cached source directly rather than completing that handoff.

Add an isolated fake-storage/delegate test that completes explicit authorization,
then acquires the cached token and refreshes it without another interactive
prompt. Cover both flow variants and omitted/rotated refresh-token responses.
Use synthetic tokens and a local endpoint only; do not authorize a real account.

## Verification for a follow-up change

Run focused tests through the shared build queue, reusing the worktree's
`target-cli` and worker limits:

```sh
agent-work run -- scripts/cargo-test -p mxr-provider-gmail --tests
agent-work run -- scripts/cargo-test -p mxr --lib -- classify_sync_error classify_log_line oauth_transport_failures handler::freshness::tests::
agent-work run -- scripts/pre-pr-rust-gate
agent-work run -- cargo build -p mxr
```

Keep any implementation separate from the reviewed Gmail source. Verify the
specific synthetic failure before changing behavior; these follow-ups do not
authorize account changes, real Gmail calls, or broader workspace builds.
