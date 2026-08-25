---
id: task-000
title: Review, merge, release, and install-verify issue 216
status: accepted
phase: preflight
depends_on: []
risk: { level: high, blast_radius: medium }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, code-review, simplify]
scope:
  base: f4bed17b62e1dae74363476cb7342e80046e6940
  head: efb68d17cacf59d0f8beed2d8d8a0e4d7061d40c
  allowed_paths:
    - crates/provider-imap/src/error.rs
    - crates/provider-imap/src/lib.rs
    - crates/provider-imap/src/session.rs
validation:
  test_commands:
    - scripts/cargo-test -p mxr-provider-imap --tests
    - cargo clippy -p mxr-provider-imap --tests -- -D warnings
    - cargo build -p mxr
  deployment:
    - merge to main
    - create a new immutable version tag
    - wait for release workflow
    - verify Homebrew
    - verify install.sh in a temporary directory
    - verify tagged cargo install in a temporary root
---

# Ship issue 216

Primary acceptance: an underreported IMAP literal cannot abort the account. Valid messages before and after the malformed UID persist; the poisoned connection is never reused.

Out of scope: connection concurrency configuration, dependency trace redaction, and credential-store behavior. Track separately.
