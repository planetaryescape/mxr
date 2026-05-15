# Changelog

## [0.5.9](https://github.com/planetaryescape/mxr/compare/v0.5.8...v0.5.9) (2026-05-15)


### Features

* [] add bridge contacts autocomplete ([7e05622](https://github.com/planetaryescape/mxr/commit/7e05622ddf6185a94b067b949cb86a36d4a55ee9))
* [] add DemoLlmProvider so demo LLM features run fully offline ([680ba7f](https://github.com/planetaryescape/mxr/commit/680ba7f8b667239dfb56d5ccd7ce0b779e2ce24d))
* [] add doctor --recompute-link-counts backfill ([06a449e](https://github.com/planetaryescape/mxr/commit/06a449e506fcc234fa93cfa44fb0645868623794))
* [] add draft-assist right-rail panel ([7ad82c1](https://github.com/planetaryescape/mxr/commit/7ad82c1208ffa1ff6a80922ad28aa75855c2794c))
* [] add has:link / has:link-heavy / has:link-none search filters ([3967083](https://github.com/planetaryescape/mxr/commit/3967083a031af41c240a8fb5a6e27d27954253e9))
* [] add sender route, screener multi-account notice, registry-driven keybindings page ([cb88186](https://github.com/planetaryescape/mxr/commit/cb881861f244daf1e9f1013d71fe1433319b0cbb))
* [] add shared web action registry foundation ([49b640f](https://github.com/planetaryescape/mxr/commit/49b640fa721988f76cf5f9c491277b4682a3eeb7))
* [] add tri-state LinkDensity classification ([c98bd72](https://github.com/planetaryescape/mxr/commit/c98bd72770f92e8ef2d66bc0508ff8607533d53a))
* [] add UpdateSavedSearch protocol request, daemon handler, and bridge route ([56257a2](https://github.com/planetaryescape/mxr/commit/56257a281c8f2899c9a4e6ff223c6b7d7182d2c8))
* [] add web account repair and refresh ([bb2565d](https://github.com/planetaryescape/mxr/commit/bb2565d35ac02f00cbf3d05c4c183487032fe22c))
* [] add web compose contact autocomplete and outbound undo ([8e09bdd](https://github.com/planetaryescape/mxr/commit/8e09bdd8b692a694ca6a5a286ad26d2751d6c56b))
* [] add web saved-search manager and search scope picker ([54225c0](https://github.com/planetaryescape/mxr/commit/54225c04f33a5048e121a2323b764458bc87418b))
* [] add Wrapped story mode, share-as-image, and refresh-contacts command ([553d48a](https://github.com/planetaryescape/mxr/commit/553d48a9aa95006160ba16e1c05cfa8f10c8899b))
* [] extend optimistic mail mutations to label/move/unsubscribe/read-and-archive ([e3619ee](https://github.com/planetaryescape/mxr/commit/e3619ee0e291f7884a24c0610107aaaeec2b9f13))
* [] extend rule actions to label/move/read-and-archive ([180338c](https://github.com/planetaryescape/mxr/commit/180338c50d4e1e993efab35ca6482253e31f2723))
* [] extract body link metrics during sync and persist link_count ([383b8cb](https://github.com/planetaryescape/mxr/commit/383b8cb01e024466f4094cc8cdb36c65eea2a2b0))
* [] inline thread summary preview and rework reply prewarm dispatcher ([796eb27](https://github.com/planetaryescape/mxr/commit/796eb2763509eeb3ce14b77aab9c1d0b6a25735f))
* [] make mxr demo sticky with stop/status/reset subcommands ([f29be6b](https://github.com/planetaryescape/mxr/commit/f29be6b3378beb6b6439dd24a1fdb6ee74de07bd))
* [] make web app installable ([ab2b04e](https://github.com/planetaryescape/mxr/commit/ab2b04eea91db45cf3d0fabef2b18389c44587e7))
* [] prompt for save destination when downloading attachments ([a235ed8](https://github.com/planetaryescape/mxr/commit/a235ed82d36b3ce83621ea45a8b7c43e8192e1b6))
* [] render tri-state link indicator on TUI and web mail rows ([ccb0820](https://github.com/planetaryescape/mxr/commit/ccb0820ad8ba5e1e49760881d8df272ba961dbb4))
* add is:owed-reply search filter and ClientKind tagging for bench fixtures ([60adaa3](https://github.com/planetaryescape/mxr/commit/60adaa3d55811fb4804f58e0f37e4ad9ee2daeff))
* add relationship-aware drafting and signatures ([0948483](https://github.com/planetaryescape/mxr/commit/094848363cb368f6674487c5df18ead87e3d79b7))
* **archive:** hybrid lexical+semantic retrieval with honest executed_mode ([29b04c5](https://github.com/planetaryescape/mxr/commit/29b04c551f74c450a050dd0de2dc902f71b3c552))
* **archive:** mxr ask with citation-validated retrieval ([7d7c558](https://github.com/planetaryescape/mxr/commit/7d7c558b95d7c05f1ccb9fc23756b689765de7c8))
* **briefing:** thread + recipient briefings with content-hash cache ([94a9018](https://github.com/planetaryescape/mxr/commit/94a90180e6665bc85143963b590ad766f2a98e3a))
* **cadence:** relationship watchlist with drift query ([55cd3e9](https://github.com/planetaryescape/mxr/commit/55cd3e9023d8f25c884795217d4d4e5a2b3093e8))
* **commitments:** extract draft commitment candidates and promote on send ([e338565](https://github.com/planetaryescape/mxr/commit/e338565d279e5618ba71d9fd355746faa1c3ebc8))
* complete platform workflow parity ([810bab6](https://github.com/planetaryescape/mxr/commit/810bab67c4000b37cc0513b254571a04b991046d))
* complete relationship intelligence surfaces ([46ffb09](https://github.com/planetaryescape/mxr/commit/46ffb096d0055602aa5724c644d466fb4e90748c))
* complete relationship workflow surfaces ([fcb1f2c](https://github.com/planetaryescape/mxr/commit/fcb1f2cd70c393aa7a092e7def3dea24db50404f))
* **core:** add DraftIntent and thread it through compose, send, and storage ([56860bf](https://github.com/planetaryescape/mxr/commit/56860bf7f15f706c5dc845383845ef4b7c1cdfb7))
* **daemon:** add user activity log recorder and CLI ([5ec1ad4](https://github.com/planetaryescape/mxr/commit/5ec1ad45d791c50381c51c35fe228ade1b330321))
* **daemon:** block sends on missing recipients, invalid addresses, and reply-all gaps ([2657ed9](https://github.com/planetaryescape/mxr/commit/2657ed96f7e10abaf48f15e0429b88c2d0990d5c))
* **decisions:** citation-backed decision log store + IPC + CLI ([8070fba](https://github.com/planetaryescape/mxr/commit/8070fba484164cedd5e41b502f4fd78b4a71771c))
* **decisions:** LLM extractor + mxr decisions rebuild subcommand ([c9dfda5](https://github.com/planetaryescape/mxr/commit/c9dfda540c5c31dd2df6587c5677ef73cd4c5994))
* **expert:** expert finder ranks answerers, not askers ([49cd981](https://github.com/planetaryescape/mxr/commit/49cd9811c09939d71ca8a900c8a1c27a5a1b6c34))
* **expert:** mxr expert CLI surface for FindExpert IPC ([5e111a7](https://github.com/planetaryescape/mxr/commit/5e111a74485e7f15e97f0c66bd57620476166325))
* isolate dev runtime identity ([e047f36](https://github.com/planetaryescape/mxr/commit/e047f36393de53034584e3335603007692f81201))
* **llm:** scaffold answer-coverage, archive-ask, decision-log, briefing, expert features ([7f0d165](https://github.com/planetaryescape/mxr/commit/7f0d165b2b25f6715be0cb78e8a283cfd6780aee))
* **outlook:** bundle client ID into release builds ([#32](https://github.com/planetaryescape/mxr/issues/32)) ([8816939](https://github.com/planetaryescape/mxr/commit/88169396e2639192a3e4f4d3abd084c66652c029))
* **owed:** list owed-reply threads with overdue ranking ([b892922](https://github.com/planetaryescape/mxr/commit/b892922599a5ddd1deedc107b9bae373660851b8))
* **safety:** LLM answer-coverage check with citation validation ([30df45f](https://github.com/planetaryescape/mxr/commit/30df45f09a1a33234e3aad35709d86b9b0ced31a))
* **safety:** scaffold deterministic pre-send safety crate ([3892d8b](https://github.com/planetaryescape/mxr/commit/3892d8b8cfc348e97facd78b97e82b216d7a0e1f))
* **safety:** single-use override tokens and send-gate audit ([6cee8de](https://github.com/planetaryescape/mxr/commit/6cee8de49bf405318a88525cb309fa0ba32e8cbb))
* **safety:** wire protocol and CLI --check for draft safety pipeline ([3a6dc89](https://github.com/planetaryescape/mxr/commit/3a6dc8928977fda6827fe65f55c4b6b3c32bd5a8))
* **safety:** wire send-time recommendation as Severity::Info hint ([c800d9a](https://github.com/planetaryescape/mxr/commit/c800d9a7ca01dbdb80046f1459a107ff4ba99081))
* **semantic:** expose chunk id, source kind, and snippet on hits ([2035661](https://github.com/planetaryescape/mxr/commit/20356611e3f3f64792d4051a7093f13a2be9d0a9))
* **send-time:** per-recipient reply-bucket optimizer + IPC + CLI ([0b9a719](https://github.com/planetaryescape/mxr/commit/0b9a7190bf60a056c0bd943dbff224a5dcda9dfc))
* ship AI-email features (pre-send safety, archive intel, timing/cadence, briefings, decisions, commitments, reply later) ([5ce96b5](https://github.com/planetaryescape/mxr/commit/5ce96b5be9eea5f5558379647c7db7d2cb5720a3))
* **store:** add user activity log schema and event log filtering ([4c9fc7b](https://github.com/planetaryescape/mxr/commit/4c9fc7ba42080583a966582f6438abff701f9235))
* **store:** include cc/bcc and reply pairs in contact rollup ([4bf6166](https://github.com/planetaryescape/mxr/commit/4bf6166ebaafa0716240bf9082523c5f82a399bb))
* **suggest:** "maybe include" recipient suggestions with Bcc-leak guard ([e865eef](https://github.com/planetaryescape/mxr/commit/e865eef7027c4c732ee4443b21d210c414113c4a))
* **suggest:** mxr suggest-recipients CLI ([8c5e085](https://github.com/planetaryescape/mxr/commit/8c5e0856e7e87a878117787f89bf520e89678718))
* support calendar email invites ([576abf6](https://github.com/planetaryescape/mxr/commit/576abf6df5fdcdfec7254ace2ae7cae5689243ae))
* **tui:** briefing modal + B-key handler for thread briefings ([823f692](https://github.com/planetaryescape/mxr/commit/823f692b99584d0cc7a384103822e2bf00cdbcf3))
* **tui:** owed-replies lens render ([81f50e9](https://github.com/planetaryescape/mxr/commit/81f50e94adaf229156317800b8737f071a767395))
* **tui:** owed-replies sidebar lens with post-send refresh ([1cd07ba](https://github.com/planetaryescape/mxr/commit/1cd07babc4721f7eaeb8e3e28c9ccac539b93f92))
* **tui:** polish command palette, send-confirm modal, diagnostics page, behaviors ([ef15d8c](https://github.com/planetaryescape/mxr/commit/ef15d8cfb2ca6c90c757d3c7cbbc880af9ad32be))
* **tui:** render safety verdict and override token in send-confirm modal ([faeb3b6](https://github.com/planetaryescape/mxr/commit/faeb3b62d548cef6d30c13bae3a5dd33a695efde))
* **tui:** show Outlook device code during auth ([#33](https://github.com/planetaryescape/mxr/issues/33)) ([1ac9c12](https://github.com/planetaryescape/mxr/commit/1ac9c1211c98dbcd7422fa1a97d4ea17fb7b75a1))
* **tui:** whois modal + W keybind on focused sender ([ad09d4f](https://github.com/planetaryescape/mxr/commit/ad09d4f2ce2702ff16b111f5f7162ced2503a59a))
* **tui:** wire compose-flow safety check + override-token send-path ([594a408](https://github.com/planetaryescape/mxr/commit/594a4082981ad256a0a889248d625379bf323ac8))
* **tui:** wire dormant-thread hint + suggest row + expert palette entry ([e4c88a4](https://github.com/planetaryescape/mxr/commit/e4c88a466347b97eae58f5c5d50cc7063e8aee26))
* **web:** add activity log route ([e988b87](https://github.com/planetaryescape/mxr/commit/e988b8794de1b6cb172b497588e3facbb1e04836))
* **web:** split diagnostics into events/logs panels, expand settings, add tests ([007683c](https://github.com/planetaryescape/mxr/commit/007683cb3bcaabfefea18a615cdcc54148b70e6f))
* **web:** surface summaries and sender context ([295a184](https://github.com/planetaryescape/mxr/commit/295a184532e1f37cc76d7232fa2edf0c7d6ce10b))
* **whois:** query-time entity explainer with no new schema ([88507e5](https://github.com/planetaryescape/mxr/commit/88507e56afef8c64810de69916484bff8065234c))
* wire activity log end-to-end and add TUI modal ([1ce2552](https://github.com/planetaryescape/mxr/commit/1ce2552c1005ee31fd325bf35b0fa16004ddcbe6))


### Bug Fixes

* **cli:** refresh and extend cli_help snapshots for AI-email subcommands ([202bd8e](https://github.com/planetaryescape/mxr/commit/202bd8e55bbc97fc15e314dfbdf4641ebe7ff691))
* **daemon:** truncate long attachment filenames ([#30](https://github.com/planetaryescape/mxr/issues/30)) ([bd2ef10](https://github.com/planetaryescape/mxr/commit/bd2ef10fa40d020900a8a8416659e21411f68c60))
* improve gmail account validation ([5238fb9](https://github.com/planetaryescape/mxr/commit/5238fb9174b57580814b4cf68a7ec69a72453c59))
* restore release CI checks ([8f60708](https://github.com/planetaryescape/mxr/commit/8f6070876d411220a33c555de79499fb324bd741))
* restore semantic release checks ([cd30269](https://github.com/planetaryescape/mxr/commit/cd3026912be48993cf3a21ea5a23241f5830ef4a))
* **snapshots:** refresh openapi spec summary for AI-email schemas ([6437cd0](https://github.com/planetaryescape/mxr/commit/6437cd06e895f777429ef6bc9ffff36bf70c2a6a))
* **store:** make migration 026 idempotent for pre-versioning backfill ([417676d](https://github.com/planetaryescape/mxr/commit/417676d8cdb5455cc1b8f073d153d8a078f51c76))


### Performance

* [] prewarm reply context on message open for instant r/a ([f0f4209](https://github.com/planetaryescape/mxr/commit/f0f4209ed174bd618a86d776f17caa209651440f))


### Refactoring

* [] migrate command palette to action registry ([65adfc9](https://github.com/planetaryescape/mxr/commit/65adfc987dfc11ae2b1c8f3df9cbc56cb591d36c))
* [] migrate global keymap to action registry ([9c7d9e6](https://github.com/planetaryescape/mxr/commit/9c7d9e6eb9e03aa2adf4e04e0878f5cb637e9ab4))
* [] migrate HelpDialog and StatusBar to action registry ([0f46da7](https://github.com/planetaryescape/mxr/commit/0f46da77cb46bf364a78c890318b58bd06801e29))
* [] remove native desktop app ([261e5f5](https://github.com/planetaryescape/mxr/commit/261e5f59937af7bc78205931037d638f865b16a9))
* [] rename client shell route ([13967c0](https://github.com/planetaryescape/mxr/commit/13967c0aa1ae69980719247bbd00afb1be63060d))
* **test-support:** promote daemon-spawning helpers from cli_journey ([6c7e2ea](https://github.com/planetaryescape/mxr/commit/6c7e2ea4298629583c516502875f651ec499301b))


### Documentation

* [] add user activity log design plan ([f653dfd](https://github.com/planetaryescape/mxr/commit/f653dfde4c9002fe7603c88c82c347ec5b9a63c6))
* [] add web parity-closure plan ([90ef1d9](https://github.com/planetaryescape/mxr/commit/90ef1d9f214512e81e06a3cced68068ddf7d0d3c))
* [] document sticky demo mode, has:link filters, and link indicator ([a977734](https://github.com/planetaryescape/mxr/commit/a9777343c3dc2fa28cbd2250f2350e2517efaab1))
* [] document web parity closure on docs site ([53eab0f](https://github.com/planetaryescape/mxr/commit/53eab0f5ee44be59b66ba618c54be3ae4310d97b))
* [] explain no native desktop app ([da61537](https://github.com/planetaryescape/mxr/commit/da61537ae33ff5ae46ada9b45b282565813172d5))
* add implementation journey context ([c222715](https://github.com/planetaryescape/mxr/commit/c2227153abed56a76b87b2acf73053f8460ce3e3))
* consolidate per-feature design docs into single files ([efaeb9a](https://github.com/planetaryescape/mxr/commit/efaeb9acf3dec54adf3c93663552fa6eff97f3a8))
* document semantic relationship workflows ([cfbf377](https://github.com/planetaryescape/mxr/commit/cfbf37778ef11aa8a423df285fc5a817a8b572a7))
* move ai-email vision out of vision subfolder ([c38c3f7](https://github.com/planetaryescape/mxr/commit/c38c3f7bc23b2aab8c19e3bf38d1752add780a50))
* refresh mxr guidance and references ([471c01d](https://github.com/planetaryescape/mxr/commit/471c01d4983b36a37ea67c7f1d229553ff4a663b))
* refresh vision handoff ([2e96231](https://github.com/planetaryescape/mxr/commit/2e96231d6069a32f56002203800e290a61a54d8e))
* **site:** add user-facing guides for activity log and AI features ([7be263e](https://github.com/planetaryescape/mxr/commit/7be263e42bf3de4c6fdfe5db352d92bd5a08d29b))
* update vision handoff status ([5017563](https://github.com/planetaryescape/mxr/commit/501756368fffaf5e63ca077d4f5d028c885bd8be))

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
