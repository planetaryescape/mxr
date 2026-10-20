# Shell-hook completion waits for inherited stderr

**Status:** open, deferred from PR #530; not a logger-rotation blocker.

The coordinator's preserved fake-mail triage ran `sleep 5 & exit 0` with a
one-second hook timeout against both installed 0.6.110 (source
`258a191c6a994626d2cff9fdda5b49061da9488a`) and S3 candidate
`33f9b600c1f94098dde28b3f2d0990e0a18bc080`. Both recorded
`success: false` and `Command 'sleep 5 & exit 0' timed out after 1s`.
The background process retains stderr after the direct shell exits; both the
baseline `wait_with_output` and the S3 concurrent drain await pipe EOF.

Evidence: [PR discussion](https://github.com/planetaryescape/mxr/pull/530#discussion_r4324148105)
and [coordinator differential triage](https://github.com/planetaryescape/mxr/pull/530#issuecomment-6289331486).
This records that triage, not a new reproduction or runtime fix in S4a.

A follow-up must define bounded post-exit draining and delivery/error policy
for success, nonzero exits, read errors, large stderr, inherited stdin and
partial delivery. Do not select an arbitrary grace period without those
cases. Descendant supervision remains outside the current shell-hook contract.
