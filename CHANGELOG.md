# Changelog

## [0.4.68](https://github.com/planetaryescape/mxr/compare/v0.4.67...v0.4.68) (2026-05-06)


### Features

* [] add Gmail-style search operators ([6b765bf](https://github.com/planetaryescape/mxr/commit/6b765bf9338bd9db6df6dbc11141480796ea6b10))
* add --dry-run to mxr send and mxr unsnooze ([d21b65b](https://github.com/planetaryescape/mxr/commit/d21b65b858840b97cc7df0efcff32547246e2713))
* store Gmail OAuth tokens in OS keychain instead of plaintext on disk ([c7aba27](https://github.com/planetaryescape/mxr/commit/c7aba273862b53ee6ea74284302b83cac5bfa9cc))
* surface body-fetch failures in TUI + Gmail attachment base64 fix ([9fd48f7](https://github.com/planetaryescape/mxr/commit/9fd48f7f851d10a721bdc7388c8a06e2f62a36cd))


### Bug Fixes

* upsert labels and envelopes by natural key to survive ID derivation change ([3d4414b](https://github.com/planetaryescape/mxr/commit/3d4414b2da247fa6533a865e04a2300331fd95bd))


### Refactoring

* track applied DB migrations in schema_migrations table ([98f6a87](https://github.com/planetaryescape/mxr/commit/98f6a8753956bf77ed95dd01ee5cb2484c7bcb5a))
* typed MxrError::RateLimited and IpcErrorKind::RateLimited ([6efddc0](https://github.com/planetaryescape/mxr/commit/6efddc007c421e181306156361fed2875a49f27f))


### Documentation

* **cli:** document undo, snooze, accounts repair/disable/remove, search operators ([c1a1e13](https://github.com/planetaryescape/mxr/commit/c1a1e1331f7c4229320bba452f7e5952cc2a71e9))
* drop ghost flags, lead with mxr accounts add for IMAP/SMTP ([58ea627](https://github.com/planetaryescape/mxr/commit/58ea627e007bc3f8465e08dcb89359aed9d49213))
* fix install instructions to match shipped reality ([1d4fbc9](https://github.com/planetaryescape/mxr/commit/1d4fbc9e8973559f115cd7b013f8685d9022213b))

## [0.4.63](https://github.com/planetaryescape/mxr/compare/v0.4.62...v0.4.63) (2026-05-04)


### Bug Fixes

* per-crate .sqlx caches and build.rs that respects DATABASE_URL so `cargo install --locked mxr` succeeds end-to-end

## [0.4.62](https://github.com/planetaryescape/mxr/compare/v0.4.61...v0.4.62) (2026-05-04)


### Bug Fixes

* default to SQLX_OFFLINE=true via build.rs in published crates so `cargo install --locked mxr` builds without a database

## [0.4.61](https://github.com/planetaryescape/mxr/compare/v0.4.60...v0.4.61) (2026-05-04)


### Bug Fixes

* topologically sort crates.io publish loop so dev-deps land before consumers

## [0.4.60](https://github.com/planetaryescape/mxr/compare/v0.4.59...v0.4.60) (2026-05-04)


### Bug Fixes

* publish mxr-test-support before its dependents in the crates.io publish loop

## [0.4.59](https://github.com/planetaryescape/mxr/compare/v0.4.58...v0.4.59) (2026-05-04)


### Bug Fixes

* add description metadata to internal crates so crates.io publish accepts them

## [0.4.58](https://github.com/planetaryescape/mxr/compare/v0.4.57...v0.4.58) (2026-05-04)


### Bug Fixes

* build artifacts for release-prepare commits even when the diff is version-only

## [0.4.57](https://github.com/planetaryescape/mxr/compare/v0.4.56...v0.4.57) (2026-05-04)


### Bug Fixes

* sync Cargo.lock to workspace version so `cargo build --locked` succeeds in release CI

## [0.4.56](https://github.com/planetaryescape/mxr/compare/v0.4.55...v0.4.56) (2026-05-04)


### Bug Fixes

* drop unused Timelike import to clear CI -D warnings ([67465a7](https://github.com/planetaryescape/mxr/commit/67465a72e5df30a33caf10e592100f4d7392c3ab))

## [0.4.55](https://github.com/planetaryescape/mxr/compare/v0.4.54...v0.4.55) (2026-05-04)


### Features

* CLI surface — non-interactive accounts add and logs --format json ([b7effc4](https://github.com/planetaryescape/mxr/commit/b7effc44c84d4b0042015c21d8201fb3cd618a30))
* email analytics — CLI commands ([0f48f63](https://github.com/planetaryescape/mxr/commit/0f48f6349e6a89be45f2a0deb02a0220d2464a8b))
* email analytics — protocol + daemon plumbing ([972612a](https://github.com/planetaryescape/mxr/commit/972612a95e44ce58d555ab6159299081d4a3d01c))
* email analytics — schema migrations + store APIs ([67a5194](https://github.com/planetaryescape/mxr/commit/67a51944805e55b2385006b7b58f07e2634af90b))
* gmail device flow for ssh and headless ([c7d2895](https://github.com/planetaryescape/mxr/commit/c7d2895a437a0c75db82ed20cf31b7384d17f599))
* send-flow correctness — sent visibility, draft state machine, idempotency ([1f0c7bb](https://github.com/planetaryescape/mxr/commit/1f0c7bb2d6055d9bc42f3274ab0d5509c02dbb97))


### Bug Fixes

* prevent silent label corruption with transactional set_message_labels ([0a6d40e](https://github.com/planetaryescape/mxr/commit/0a6d40e8a05d4f2ab2bb56e3473d0dcd6e613756))


### Documentation

* install path, quick-start, troubleshooting, release-please drift ([b5e982d](https://github.com/planetaryescape/mxr/commit/b5e982d305970a0c6931a31a0e304bba1f56abf3))

## [0.4.17](https://github.com/planetaryescape/mxr/compare/v0.4.16...v0.4.17) (2026-03-24)


### Bug Fixes

* collapse mxr into a single publishable package ([b83b9e6](https://github.com/planetaryescape/mxr/commit/b83b9e6bded098bc3a0e15a758ce91b7beac7370))

## [0.4.16](https://github.com/planetaryescape/mxr/compare/v0.4.15...v0.4.16) (2026-03-23)


### Bug Fixes

* make release-please handle workspace crates ([cc81ec9](https://github.com/planetaryescape/mxr/commit/cc81ec97d4ece363ba3317c5b187200391b09099))
