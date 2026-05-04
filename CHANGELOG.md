# Changelog

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
