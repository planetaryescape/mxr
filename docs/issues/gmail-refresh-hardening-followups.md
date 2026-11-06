# Gmail refresh hardening: known limits

**Status:** known limits, accepted · **Severity:** low

The Gmail refresh hardening follow-ups are resolved, the last one (doctor's
log dedupe matching the account) on 2026-11-05. Three limits remain by
choice. Each says what would make it worth changing.

## Concurrent API calls each ask the token endpoint while it throttles

A token-endpoint 429 carries its `Retry-After` to the sync backoff, but
concurrent API calls inside one sync pass each still ask the token endpoint
once while it is throttling, because the token source does not remember the
throttle between calls. The pass fails on the first error and the daemon backs
off, so the cost is a few extra 429s. A `throttled_until` on the cached source
would remove them if that ever shows up in logs.

## Server rejection text containing a network word classifies as network

A server's rejection text that itself mentions a network word (a Dovecot
`PRIVACYREQUIRED` message mentioning SSL/TLS connections) classifies as a
network failure in sync health and doctor. Narrowing the keywords further
needs structured provider errors, not more string matching.

## The outer timeout does not preempt a blocked token-cache write

On the daemon's multi-thread runtime a stuck keychain write pins one worker
and leaves the rest running (measured: 13 ms heartbeat gap with a 600 ms
blocking write), but the 100 ms guard around `access_token()` is not enforced
for that step. The macOS keychain path disables user interaction, so it
returns promptly, and no hang has been observed. Moving the write to
`spawn_blocking` would restore the guard but must keep serialization and token
rotation correct (a timed-out caller must not let an older write land after a
newer token). Revisit if a Linux Secret Service hang is ever reported.
`a_blocked_cache_write_leaves_the_rest_of_the_runtime_running` pins the part
that holds.
