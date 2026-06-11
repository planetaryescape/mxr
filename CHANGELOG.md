# Changelog

## [0.5.62](https://github.com/planetaryescape/mxr/compare/v0.5.61...v0.5.62) (2026-06-11)


### Features

* issue a safety override token in the blocked-send error ([abe45ed](https://github.com/planetaryescape/mxr/commit/abe45ed4efa66d713b9e4c35501eb5eee3c71b1a))
* per-action destructive scopes for agent profiles ([1577177](https://github.com/planetaryescape/mxr/commit/1577177b00c0ca716d0883502b0e5427b34ca199))
* persist mutation job history across daemon restarts ([02d5b43](https://github.com/planetaryescape/mxr/commit/02d5b434481eceb548eb79a3d8fcd9a02f2cc4e4))
* reply to the Reply-To address when the sender set one ([af02b7c](https://github.com/planetaryescape/mxr/commit/af02b7c1c196182e2d30411f576b3fb0c0ed3b36))
* surface scheduled sends lost to a crash mid-flush ([d3157b4](https://github.com/planetaryescape/mxr/commit/d3157b47f108b89c7d44b2383340c9220af5e6c5))
* tell users when a mutation's undo could not be recorded ([be880f5](https://github.com/planetaryescape/mxr/commit/be880f50206479f2b562b0485c7d710f20b6446d))


### Bug Fixes

* detach sync on timeout instead of cancelling mid-flight ([67c1329](https://github.com/planetaryescape/mxr/commit/67c132996c2e6d7e7559ffd4fee462fd5d34ae1c))
* signal clients to resync when their event stream lags ([eae6756](https://github.com/planetaryescape/mxr/commit/eae675626de1060070a7600fd2288d94906cd065))
* stop silently dating/attributing mail to wrong values ([f70b52d](https://github.com/planetaryescape/mxr/commit/f70b52d22d7737a70920dfd1dbfb79ca04e27266))
* **web:** handle SyncError, ReminderTriggered, and reconciliation events ([21528ba](https://github.com/planetaryescape/mxr/commit/21528baf3d1eb47e620cd531eb92fe60929ceb4b))


### Refactoring

* classify every IPC request exhaustively for safety policy ([83c5a80](https://github.com/planetaryescape/mxr/commit/83c5a8010858e64dd639753728c7cbda05081f8f))
* consolidate the parse+schema+build query idiom ([9ba92e1](https://github.com/planetaryescape/mxr/commit/9ba92e1100bf448f924dcb438ef044be71d892f5))
* log best-effort cleanup failures instead of swallowing them ([72e2365](https://github.com/planetaryescape/mxr/commit/72e2365f07259c720628b3d66332f3e3dcf10c6d))
* **web:** guard RightRail attachment payload instead of casting ([21ce202](https://github.com/planetaryescape/mxr/commit/21ce202110644aac87d5b1d40f36de40ed2411cd))

## [0.5.61](https://github.com/planetaryescape/mxr/compare/v0.5.60...v0.5.61) (2026-06-06)


### Bug Fixes

* accept completed zero-change sync status in release smoke ([8fdeb8c](https://github.com/planetaryescape/mxr/commit/8fdeb8cbe19a2e3d1d72f15aef17387149d563ce))

## [0.5.60](https://github.com/planetaryescape/mxr/compare/v0.5.59...v0.5.60) (2026-06-06)


### Bug Fixes

* prevent stuck sync and mutation rollback ([df3f90a](https://github.com/planetaryescape/mxr/commit/df3f90aff4ec195d0eb89db16cd373e795c3d9d6))

## [0.5.59](https://github.com/planetaryescape/mxr/compare/v0.5.58...v0.5.59) (2026-06-06)


### Bug Fixes

* restore workspace ci after v1 merge ([fee262c](https://github.com/planetaryescape/mxr/commit/fee262c3a8bc03d9ebea11ab1ebb9ef6fc7b514d))

## [0.5.58](https://github.com/planetaryescape/mxr/compare/v0.5.57...v0.5.58) (2026-06-05)


### Features

* relationship-aware AI drafting, cross-client parity, compose redesign ([#58](https://github.com/planetaryescape/mxr/issues/58)) ([1674dd6](https://github.com/planetaryescape/mxr/commit/1674dd635602fa34820467573a5a168644c0e363))

## [0.5.57](https://github.com/planetaryescape/mxr/compare/v0.5.56...v0.5.57) (2026-06-04)


### Bug Fixes

* track TUI summary requests per thread ([#55](https://github.com/planetaryescape/mxr/issues/55)) ([676fd40](https://github.com/planetaryescape/mxr/commit/676fd40598bf65928fae54e064fc922f9ac5b301))

## [0.5.56](https://github.com/planetaryescape/mxr/compare/v0.5.55...v0.5.56) (2026-06-03)


### Bug Fixes

* satisfy cli journey clippy ([d22ab10](https://github.com/planetaryescape/mxr/commit/d22ab109662903977b92d7923e2cabf326d48f28))

## [0.5.55](https://github.com/planetaryescape/mxr/compare/v0.5.54...v0.5.55) (2026-06-03)


### Bug Fixes

* clean up release clippy lints ([ed75ea4](https://github.com/planetaryescape/mxr/commit/ed75ea48a526e0ea781709410cf21670c4db0a72))

## [0.5.54](https://github.com/planetaryescape/mxr/compare/v0.5.53...v0.5.54) (2026-06-03)


### Bug Fixes

* satisfy release ci checks ([94f3fe6](https://github.com/planetaryescape/mxr/commit/94f3fe675420885fa40a062cbdab83700b60b172))

## [0.5.53](https://github.com/planetaryescape/mxr/compare/v0.5.52...v0.5.53) (2026-06-03)


### Bug Fixes

* update release smoke coverage for paginated search output ([dd916f8](https://github.com/planetaryescape/mxr/commit/dd916f83a3a01055bcb7340d542d56c748f26426))

## [0.5.52](https://github.com/planetaryescape/mxr/compare/v0.5.51...v0.5.52) (2026-06-03)


### Features

* **cli:** count --format plain, thread/message count parity, unsubscribe --dry-run preflight ([109745d](https://github.com/planetaryescape/mxr/commit/109745d0b1b9a330f6e5c5f3e54a29cb41e70f88))
* **mutations:** chunked/async large-batch jobs surface across clients ([a1d2734](https://github.com/planetaryescape/mxr/commit/a1d2734e42eec473e4edae8956d94f97255697ea))
* **rules:** chained rule actions + atomic route verb across clients ([b31b7d1](https://github.com/planetaryescape/mxr/commit/b31b7d167f2cf694c79c4f6e5e98ba39903a2a49))
* **search:** --group-by sender aggregation across CLI/TUI/web ([632af4b](https://github.com/planetaryescape/mxr/commit/632af4b1f3847899a466d622859e152786db054e))
* **summarize:** lead thread summaries with a strict triage verdict line ([072414c](https://github.com/planetaryescape/mxr/commit/072414cd15760511cfdf8f7ea9d428bafc20355d))
* **triage:** cached triage-signal surface across CLI/TUI/web with store cache ([ebdd93b](https://github.com/planetaryescape/mxr/commit/ebdd93b257c8674caee5522da32f5b361867b060))
* **unsubscribe:** --purge unsubscribe + footprint clear with dry-run preview ([861d5e5](https://github.com/planetaryescape/mxr/commit/861d5e52f235504473fd0cb933d6ce87af451dac))


### Bug Fixes

* route mutation integration ([9a67487](https://github.com/planetaryescape/mxr/commit/9a67487b6261dcb1e3e00d42a38ab5632f72b5f9))
* search count pagination integration ([fced6eb](https://github.com/planetaryescape/mxr/commit/fced6ebb6bd0460ca8aad2952b59e59cbe5b0c95))
* **reader:** readable HTML-to-text fallback in reader view ([935a64f](https://github.com/planetaryescape/mxr/commit/935a64f2807505361f0c1f8606053aa3784b55ff))
* **search:** lift tantivy result ceiling + offset pagination ([873a851](https://github.com/planetaryescape/mxr/commit/873a851a0aeb8f8e058375c00eb326c5156f21f9))


### Documentation

* clarify subscriptions --rank opened_count semantics ([dd60f95](https://github.com/planetaryescape/mxr/commit/dd60f95174ececf704b2caf1e5d2f14eb55162f6))
* record triage cli gaps plan ([00f4a18](https://github.com/planetaryescape/mxr/commit/00f4a18d857e4d5a33161d5a5d91d264f1dfab90))

## [0.5.51](https://github.com/planetaryescape/mxr/compare/v0.5.50...v0.5.51) (2026-05-31)


### Bug Fixes

* cover dbus in workflow checks ([a39b31b](https://github.com/planetaryescape/mxr/commit/a39b31bd4181744ee11fcaca090160c0b5ce24bc))
* **keychain:** enable a Linux keyring backend so credentials persist ([7fbd5af](https://github.com/planetaryescape/mxr/commit/7fbd5afcd9d1947962c54899b6bdd1a422d5f196)), closes [#45](https://github.com/planetaryescape/mxr/issues/45)
* **provider-imap:** skip non-selectable folders; don't abort sync on [Gmail] ([d03d3ab](https://github.com/planetaryescape/mxr/commit/d03d3ab7098e1fc2d52fba1ffea3d33e6b03d176)), closes [#45](https://github.com/planetaryescape/mxr/issues/45)
* repair Linux keyring CI ([201a1cd](https://github.com/planetaryescape/mxr/commit/201a1cd28191a824d96e94b1c7c2bd77f94b1a50))

## [0.5.50](https://github.com/planetaryescape/mxr/compare/v0.5.49...v0.5.50) (2026-05-31)


### Features

* [] add bridge contacts autocomplete ([7e05622](https://github.com/planetaryescape/mxr/commit/7e05622ddf6185a94b067b949cb86a36d4a55ee9))
* [] add daemon web bridge ([ec4894a](https://github.com/planetaryescape/mxr/commit/ec4894aeb258466c963244de02596d2d7b00841b))
* [] add debug action trace ([977b323](https://github.com/planetaryescape/mxr/commit/977b323321307811cd8978b8ec11acbb2e40c75a))
* [] add DemoLlmProvider so demo LLM features run fully offline ([680ba7f](https://github.com/planetaryescape/mxr/commit/680ba7f8b667239dfb56d5ccd7ce0b779e2ce24d))
* [] add desktop shell for mxr ([5b68d5e](https://github.com/planetaryescape/mxr/commit/5b68d5e35eede463c1f0d37a167377af445a0634))
* [] add doctor --recompute-link-counts backfill ([06a449e](https://github.com/planetaryescape/mxr/commit/06a449e506fcc234fa93cfa44fb0645868623794))
* [] add draft-assist right-rail panel ([7ad82c1](https://github.com/planetaryescape/mxr/commit/7ad82c1208ffa1ff6a80922ad28aa75855c2794c))
* [] add Gmail-style search operators ([6b765bf](https://github.com/planetaryescape/mxr/commit/6b765bf9338bd9db6df6dbc11141480796ea6b10))
* [] add has:link / has:link-heavy / has:link-none search filters ([3967083](https://github.com/planetaryescape/mxr/commit/3967083a031af41c240a8fb5a6e27d27954253e9))
* [] add sender route, screener multi-account notice, registry-driven keybindings page ([cb88186](https://github.com/planetaryescape/mxr/commit/cb881861f244daf1e9f1013d71fe1433319b0cbb))
* [] add shared web action registry foundation ([49b640f](https://github.com/planetaryescape/mxr/commit/49b640fa721988f76cf5f9c491277b4682a3eeb7))
* [] add tri-state LinkDensity classification ([c98bd72](https://github.com/planetaryescape/mxr/commit/c98bd72770f92e8ef2d66bc0508ff8607533d53a))
* [] add UpdateSavedSearch protocol request, daemon handler, and bridge route ([56257a2](https://github.com/planetaryescape/mxr/commit/56257a281c8f2899c9a4e6ff223c6b7d7182d2c8))
* [] add web account repair and refresh ([bb2565d](https://github.com/planetaryescape/mxr/commit/bb2565d35ac02f00cbf3d05c4c183487032fe22c))
* [] add web compose contact autocomplete and outbound undo ([8e09bdd](https://github.com/planetaryescape/mxr/commit/8e09bdd8b692a694ca6a5a286ad26d2751d6c56b))
* [] add web saved-search manager and search scope picker ([54225c0](https://github.com/planetaryescape/mxr/commit/54225c04f33a5048e121a2323b764458bc87418b))
* [] add Wrapped story mode, share-as-image, and refresh-contacts command ([553d48a](https://github.com/planetaryescape/mxr/commit/553d48a9aa95006160ba16e1c05cfa8f10c8899b))
* [] expand bridge and tui parity ([47d24ac](https://github.com/planetaryescape/mxr/commit/47d24ac5303d3a80b539aaf44a1873a21988a837))
* [] extend optimistic mail mutations to label/move/unsubscribe/read-and-archive ([e3619ee](https://github.com/planetaryescape/mxr/commit/e3619ee0e291f7884a24c0610107aaaeec2b9f13))
* [] extend rule actions to label/move/read-and-archive ([180338c](https://github.com/planetaryescape/mxr/commit/180338c50d4e1e993efab35ca6482253e31f2723))
* [] extract body link metrics during sync and persist link_count ([383b8cb](https://github.com/planetaryescape/mxr/commit/383b8cb01e024466f4094cc8cdb36c65eea2a2b0))
* [] finish desktop workbench ([33c2eb1](https://github.com/planetaryescape/mxr/commit/33c2eb16680068912f60bdddade047f6c8b309f9))
* [] harden mailbox html rendering ([cc3d60a](https://github.com/planetaryescape/mxr/commit/cc3d60a3325ba66bafd8d0f202862e3cb6a491fe))
* [] harden semantic ingest lifecycle ([a1518a3](https://github.com/planetaryescape/mxr/commit/a1518a36c73f3caefd25f5394e7e08aa839bc7ef))
* [] inline thread summary preview and rework reply prewarm dispatcher ([796eb27](https://github.com/planetaryescape/mxr/commit/796eb2763509eeb3ce14b77aab9c1d0b6a25735f))
* [] make mxr demo sticky with stop/status/reset subcommands ([f29be6b](https://github.com/planetaryescape/mxr/commit/f29be6b3378beb6b6439dd24a1fdb6ee74de07bd))
* [] make web app installable ([ab2b04e](https://github.com/planetaryescape/mxr/commit/ab2b04eea91db45cf3d0fabef2b18389c44587e7))
* [] prompt for save destination when downloading attachments ([a235ed8](https://github.com/planetaryescape/mxr/commit/a235ed82d36b3ce83621ea45a8b7c43e8192e1b6))
* [] refresh desktop workbench shell ([29cd7a1](https://github.com/planetaryescape/mxr/commit/29cd7a15ca3660192bc4391a79ec1747c1a5df55))
* [] render tri-state link indicator on TUI and web mail rows ([ccb0820](https://github.com/planetaryescape/mxr/commit/ccb0820ad8ba5e1e49760881d8df272ba961dbb4))
* [] stabilize daemon desktop protocol ([dd26be3](https://github.com/planetaryescape/mxr/commit/dd26be302557e66a0d8514aa7e55ac4511b4118c))
* [] tighten desktop tui parity ([7e0f212](https://github.com/planetaryescape/mxr/commit/7e0f212d5b5e40f405e9364c0f4251c945a34be7))
* add --dry-run to mxr send and mxr unsnooze ([d21b65b](https://github.com/planetaryescape/mxr/commit/d21b65b858840b97cc7df0efcff32547246e2713))
* add config edit/get/set CLI commands ([b9876e4](https://github.com/planetaryescape/mxr/commit/b9876e4b5549c64813f63ac7f56b73c5be9dc24e))
* add config-inclusive reset mode ([8babff5](https://github.com/planetaryescape/mxr/commit/8babff59fcf355abe45192c2725ae01b6e3223c0))
* add hybrid semantic search ([0d01d9f](https://github.com/planetaryescape/mxr/commit/0d01d9f945909653eda694d392fa49d38106e6d8))
* add is:owed-reply search filter and ClientKind tagging for bench fixtures ([60adaa3](https://github.com/planetaryescape/mxr/commit/60adaa3d55811fb4804f58e0f37e4ad9ee2daeff))
* add notification chimes ([56fdb7b](https://github.com/planetaryescape/mxr/commit/56fdb7bac47cc321c6084c7fbe4a5c70332fd14e))
* add relationship-aware drafting and signatures ([0948483](https://github.com/planetaryescape/mxr/commit/094848363cb368f6674487c5df18ead87e3d79b7))
* add safe local-state reset command ([9a862db](https://github.com/planetaryescape/mxr/commit/9a862db535c95b2ad113710204e2b60e2537a739))
* add scrollable account test details modal in TUI ([6333fb0](https://github.com/planetaryescape/mxr/commit/6333fb0b812e5a8a754197ee4142b887a049c12e))
* **analytics:** cut decoration, surface ratios and distribution shapes ([d47f761](https://github.com/planetaryescape/mxr/commit/d47f7618f67cf118436e88e4aff1163de85dac98))
* **analytics:** dashboard redesign across all 6 tabs + stale-while-revalidate cache ([dde446f](https://github.com/planetaryescape/mxr/commit/dde446f9b19cf480df989588198961af9e3782d6))
* **analytics:** perf, UX overhaul; release ships semantic-local ([e8081b2](https://github.com/planetaryescape/mxr/commit/e8081b28adcb3712627af9f93daa8d89884f0ba8))
* **archive:** hybrid lexical+semantic retrieval with honest executed_mode ([29b04c5](https://github.com/planetaryescape/mxr/commit/29b04c551f74c450a050dd0de2dc902f71b3c552))
* **archive:** mxr ask with citation-validated retrieval ([7d7c558](https://github.com/planetaryescape/mxr/commit/7d7c558b95d7c05f1ccb9fc23756b689765de7c8))
* **bridge:** slice 1 — feature-gated ToSchema on protocol ([74bf031](https://github.com/planetaryescape/mxr/commit/74bf0315a1e5a4619634cdbe55c7bd98f9cba085))
* **bridge:** slice 2 — utoipa scaffold + openapi.json + Swagger UI ([aec9a3b](https://github.com/planetaryescape/mxr/commit/aec9a3b84d24c1bb7fbb8e6d7dbb454654bd95f9))
* **bridge:** slice 3 — bucket all 47 routes under /api/v1/* + 308 shim ([a0a32a2](https://github.com/planetaryescape/mxr/commit/a0a32a27fe10e6b80f33698ab847039eb708bf48))
* **bridge:** slice 4 — auth hardening, /health, host & CORS allowlists ([a1805f7](https://github.com/planetaryescape/mxr/commit/a1805f76ca98f71691585a2a12e0da2c902884a6))
* **bridge:** slice 5 — daemon-hosted bridge as managed task ([e93f053](https://github.com/planetaryescape/mxr/commit/e93f053c843562ab299a23676ec26ebe3d5395c8))
* **bridge:** slice 6 — close protocol coverage gap (~30 new routes) ([d25b65e](https://github.com/planetaryescape/mxr/commit/d25b65edf7643066ad7db212d9c4d39161c4bf77))
* **bridge:** slice 7 — integration harness + OpenAPI conformance CI ([86ac37b](https://github.com/planetaryescape/mxr/commit/86ac37ba9f96a78f5a8310e828c96f8983981f0d))
* **bridge:** slice 8 — desktop app forward-compat + codegen tooling ([5e44f35](https://github.com/planetaryescape/mxr/commit/5e44f35806b52975816c94ee73be75c009f4a6fe))
* **briefing:** thread + recipient briefings with content-hash cache ([94a9018](https://github.com/planetaryescape/mxr/commit/94a90180e6665bc85143963b590ad766f2a98e3a))
* **cadence:** relationship watchlist with drift query ([55cd3e9](https://github.com/planetaryescape/mxr/commit/55cd3e9023d8f25c884795217d4d4e5a2b3093e8))
* calendar invites lens and web page with inline RSVP ([3e443f7](https://github.com/planetaryescape/mxr/commit/3e443f794628bd0164a393bf8ae928a88b78167e))
* CLI surface — non-interactive accounts add and logs --format json ([b7effc4](https://github.com/planetaryescape/mxr/commit/b7effc44c84d4b0042015c21d8201fb3cd618a30))
* **commitments:** extract draft commitment candidates and promote on send ([e338565](https://github.com/planetaryescape/mxr/commit/e338565d279e5618ba71d9fd355746faa1c3ebc8))
* complete platform workflow parity ([810bab6](https://github.com/planetaryescape/mxr/commit/810bab67c4000b37cc0513b254571a04b991046d))
* complete relationship intelligence surfaces ([46ffb09](https://github.com/planetaryescape/mxr/commit/46ffb096d0055602aa5724c644d466fb4e90748c))
* complete relationship workflow surfaces ([fcb1f2c](https://github.com/planetaryescape/mxr/commit/fcb1f2cd70c393aa7a092e7def3dea24db50404f))
* **core:** add DraftIntent and thread it through compose, send, and storage ([56860bf](https://github.com/planetaryescape/mxr/commit/56860bf7f15f706c5dc845383845ef4b7c1cdfb7))
* custom IMAP keywords on flags (MSP Phase E) ([b23a0d8](https://github.com/planetaryescape/mxr/commit/b23a0d8ea7628bcbed131c4a5c8ff569b0411e8d))
* **daemon:** add user activity log recorder and CLI ([5ec1ad4](https://github.com/planetaryescape/mxr/commit/5ec1ad45d791c50381c51c35fe228ade1b330321))
* **daemon:** block sends on missing recipients, invalid addresses, and reply-all gaps ([2657ed9](https://github.com/planetaryescape/mxr/commit/2657ed96f7e10abaf48f15e0429b88c2d0990d5c))
* **daemon:** IMAP IDLE wake-up framework + FakeProvider hook ([3c2a616](https://github.com/planetaryescape/mxr/commit/3c2a61640713a230c61ef72e1138d5fee53019d6))
* **decisions:** citation-backed decision log store + IPC + CLI ([8070fba](https://github.com/planetaryescape/mxr/commit/8070fba484164cedd5e41b502f4fd78b4a71771c))
* **decisions:** LLM extractor + mxr decisions rebuild subcommand ([c9dfda5](https://github.com/planetaryescape/mxr/commit/c9dfda540c5c31dd2df6587c5677ef73cd4c5994))
* **deliveries:** track packages from inbound mail across CLI, web, and TUI ([a49a917](https://github.com/planetaryescape/mxr/commit/a49a917f893e439ec55530fa1391e04e7c030fc2))
* **demo:** seed shipping mail so the demo profile shows deliveries ([d8a39e0](https://github.com/planetaryescape/mxr/commit/d8a39e094cbd258e8e325861d514729cf0034d5e))
* email analytics — CLI commands ([0f48f63](https://github.com/planetaryescape/mxr/commit/0f48f6349e6a89be45f2a0deb02a0220d2464a8b))
* email analytics — protocol + daemon plumbing ([972612a](https://github.com/planetaryescape/mxr/commit/972612a95e44ce58d555ab6159299081d4a3d01c))
* email analytics — schema migrations + store APIs ([67a5194](https://github.com/planetaryescape/mxr/commit/67a51944805e55b2385006b7b58f07e2634af90b))
* **expert:** expert finder ranks answerers, not askers ([49cd981](https://github.com/planetaryescape/mxr/commit/49cd9811c09939d71ca8a900c8a1c27a5a1b6c34))
* **expert:** mxr expert CLI surface for FindExpert IPC ([5e111a7](https://github.com/planetaryescape/mxr/commit/5e111a74485e7f15e97f0c66bd57620476166325))
* extract mail-threading into standalone crate ([222a00b](https://github.com/planetaryescape/mxr/commit/222a00bc780597b4981d55f51ac6cece4b9cd9db))
* gmail device flow for ssh and headless ([c7d2895](https://github.com/planetaryescape/mxr/commit/c7d2895a437a0c75db82ed20cf31b7384d17f599))
* improve tui help and account setup ([e06c0ab](https://github.com/planetaryescape/mxr/commit/e06c0ab85ce0e55747a723cb9d76d80681287780))
* inline calendar reply with localization scaffold ([476dbb6](https://github.com/planetaryescape/mxr/commit/476dbb646664c48af2aa12ff49e1eb3a88be3da3))
* **invites:** re-hydrate attachment-only invites in `mxr invites backfill` ([52eca71](https://github.com/planetaryescape/mxr/commit/52eca71dd53fc195337fd3997c730b76020445b7))
* isolate dev runtime identity ([e047f36](https://github.com/planetaryescape/mxr/commit/e047f36393de53034584e3335603007692f81201))
* **llm:** scaffold answer-coverage, archive-ask, decision-log, briefing, expert features ([7f0d165](https://github.com/planetaryescape/mxr/commit/7f0d165b2b25f6715be0cb78e8a283cfd6780aee))
* mxr undo CLI + Tantivy reindex on restore ([88c5d88](https://github.com/planetaryescape/mxr/commit/88c5d880f1ba29de60bf5a4773d704d2cb8455f2))
* mxr wrapped + mxr storage --by message ([de82195](https://github.com/planetaryescape/mxr/commit/de82195c3aeaf283a560f162584ab92469550cd1))
* **outlook:** bundle client ID into release builds ([#32](https://github.com/planetaryescape/mxr/issues/32)) ([8816939](https://github.com/planetaryescape/mxr/commit/88169396e2639192a3e4f4d3abd084c66652c029))
* **owed:** list owed-reply threads with overdue ranking ([b892922](https://github.com/planetaryescape/mxr/commit/b892922599a5ddd1deedc107b9bae373660851b8))
* persistent search workspace ([033bca7](https://github.com/planetaryescape/mxr/commit/033bca7894d29b6d7b0aae44db0db92bbdf2be1b))
* phase 1-4 — triage, drafts, llm scaffolding, sender view, screener ([fd1bf60](https://github.com/planetaryescape/mxr/commit/fd1bf60370c91f30ede10a99fa5d5f187dcc66b0))
* prepare list-unsubscribe for external publish ([34cac5e](https://github.com/planetaryescape/mxr/commit/34cac5e5c898d5fd5691b495bf54e2b0d72b7ce5))
* prepare mail-query for external publish ([8e524ef](https://github.com/planetaryescape/mxr/commit/8e524eff93298912c83816b47ccb75a5546acfa8))
* prepare mail-threading for external publish ([3bae16d](https://github.com/planetaryescape/mxr/commit/3bae16da500421178db9a65719efdc40e753fec1))
* prepare mailbox-formats for external publish ([3692a42](https://github.com/planetaryescape/mxr/commit/3692a428dd9efb2e4feb69c6c5faf25aebdf01d2))
* **provider-imap:** real IMAP IDLE protocol wiring ([beb2d42](https://github.com/planetaryescape/mxr/commit/beb2d42561e1ac5baf852bbca5e44de4e7957d89))
* refresh tui ux ([e0699bc](https://github.com/planetaryescape/mxr/commit/e0699bc130fcea06877baf94821ea6535a57d5cd))
* **safety:** LLM answer-coverage check with citation validation ([30df45f](https://github.com/planetaryescape/mxr/commit/30df45f09a1a33234e3aad35709d86b9b0ced31a))
* **safety:** scaffold deterministic pre-send safety crate ([3892d8b](https://github.com/planetaryescape/mxr/commit/3892d8b8cfc348e97facd78b97e82b216d7a0e1f))
* **safety:** single-use override tokens and send-gate audit ([6cee8de](https://github.com/planetaryescape/mxr/commit/6cee8de49bf405318a88525cb309fa0ba32e8cbb))
* **safety:** wire protocol and CLI --check for draft safety pipeline ([3a6dc89](https://github.com/planetaryescape/mxr/commit/3a6dc8928977fda6827fe65f55c4b6b3c32bd5a8))
* **safety:** wire send-time recommendation as Severity::Info hint ([c800d9a](https://github.com/planetaryescape/mxr/commit/c800d9a7ca01dbdb80046f1459a107ff4ba99081))
* scope cli commands by account ([098423a](https://github.com/planetaryescape/mxr/commit/098423a7021bdd3633867b7a99e4fb3e0bf5045a))
* self-healing analytics with live progress feedback ([82021fc](https://github.com/planetaryescape/mxr/commit/82021fc7ef91aae348c799f776fc600877f73df2))
* **semantic:** expose chunk id, source kind, and snippet on hits ([2035661](https://github.com/planetaryescape/mxr/commit/20356611e3f3f64792d4051a7093f13a2be9d0a9))
* send-flow correctness — sent visibility, draft state machine, idempotency ([1f0c7bb](https://github.com/planetaryescape/mxr/commit/1f0c7bb2d6055d9bc42f3274ab0d5509c02dbb97))
* **send-time:** per-recipient reply-bucket optimizer + IPC + CLI ([0b9a719](https://github.com/planetaryescape/mxr/commit/0b9a7190bf60a056c0bd943dbff224a5dcda9dfc))
* ship AI-email features (pre-send safety, archive intel, timing/cadence, briefings, decisions, commitments, reply later) ([5ce96b5](https://github.com/planetaryescape/mxr/commit/5ce96b5be9eea5f5558379647c7db7d2cb5720a3))
* ship CLI v1 — compose journeys, mutation coherence, IMAP UIDPLUS safety ([68a96d8](https://github.com/planetaryescape/mxr/commit/68a96d83bc421670b9e134130b24f07b42bca4fe))
* ship local-first docs and mutation history ([f7a1c77](https://github.com/planetaryescape/mxr/commit/f7a1c77d55d507282fd46a415a34754178e13191))
* ship semantic search and diagnostics overhaul ([c4ab988](https://github.com/planetaryescape/mxr/commit/c4ab988312a1732b2cd8d90b06184c17a3cec679))
* ship subscriptions mailbox ([28e0746](https://github.com/planetaryescape/mxr/commit/28e07467cbb8613c2f0b7d2df0031d07753b1e74))
* store Gmail OAuth tokens in OS keychain instead of plaintext on disk ([c7aba27](https://github.com/planetaryescape/mxr/commit/c7aba273862b53ee6ea74284302b83cac5bfa9cc))
* **store:** add user activity log schema and event log filtering ([4c9fc7b](https://github.com/planetaryescape/mxr/commit/4c9fc7ba42080583a966582f6438abff701f9235))
* **store:** include cc/bcc and reply pairs in contact rollup ([4bf6166](https://github.com/planetaryescape/mxr/commit/4bf6166ebaafa0716240bf9082523c5f82a399bb))
* **suggest:** "maybe include" recipient suggestions with Bcc-leak guard ([e865eef](https://github.com/planetaryescape/mxr/commit/e865eef7027c4c732ee4443b21d210c414113c4a))
* **suggest:** mxr suggest-recipients CLI ([8c5e085](https://github.com/planetaryescape/mxr/commit/8c5e0856e7e87a878117787f89bf520e89678718))
* support calendar email invites ([576abf6](https://github.com/planetaryescape/mxr/commit/576abf6df5fdcdfec7254ace2ae7cae5689243ae))
* surface body-fetch failures in TUI + Gmail attachment base64 fix ([9fd48f7](https://github.com/planetaryescape/mxr/commit/9fd48f7f851d10a721bdc7388c8a06e2f62a36cd))
* Thread.message_ids + threads_changed delta (MSP Phase F) ([13ea42e](https://github.com/planetaryescape/mxr/commit/13ea42e8c747ea2c903bc7c31daaf23afad83774))
* TUI polish, theme presets, snapshot tests, and remaining work ([5ed4622](https://github.com/planetaryescape/mxr/commit/5ed462291c472a4bf26e97ec070d458af6cd4bfa))
* **tui:** account repair from accounts view ([cf40be4](https://github.com/planetaryescape/mxr/commit/cf40be491594afb38a9250fd2f58c0cf16c9dbdc))
* **tui:** add centralized theme system, remove all hardcoded colors ([62c0d09](https://github.com/planetaryescape/mxr/commit/62c0d0923c247785b9e5ac72b62f48efb9ffbf4d))
* **tui:** add CLI-equivalent analytics views to TUI ([e034f1a](https://github.com/planetaryescape/mxr/commit/e034f1a0459ba8a437c725820f6feed1bbc2dbcb))
* **tui:** add URL picker modal (L key) with labeled links ([793c73c](https://github.com/planetaryescape/mxr/commit/793c73cd8b48f480a9167e1976fa2537e66cc0d5))
* **tui:** briefing modal + B-key handler for thread briefings ([823f692](https://github.com/planetaryescape/mxr/commit/823f692b99584d0cc7a384103822e2bf00cdbcf3))
* **tui:** centralized async error surfacing — UserError ring buffer + reporter ([24b6c5d](https://github.com/planetaryescape/mxr/commit/24b6c5de0b91944e20ced309e640c32f93e94c8d))
* **tui:** complete UI overhaul phases 2-10 ([a0da9e2](https://github.com/planetaryescape/mxr/commit/a0da9e256e640b1c84f606fb733447af1416d5fc))
* **tui:** four analytics views unified in one Analytics screen ([712921e](https://github.com/planetaryescape/mxr/commit/712921e1252b52d81051b571433072dcf8615e85))
* **tui:** highlight URLs in message body with blue underline ([4b7b97f](https://github.com/planetaryescape/mxr/commit/4b7b97fc88db334a8e49003c87d13d408a4b55fa))
* **tui:** HTML view polish — clearer placeholders + scroll on toggle ([fb0cfc1](https://github.com/planetaryescape/mxr/commit/fb0cfc13a3de15c08a74a6a400f83848d746ddab))
* **tui:** owed-replies lens render ([81f50e9](https://github.com/planetaryescape/mxr/commit/81f50e94adaf229156317800b8737f071a767395))
* **tui:** owed-replies sidebar lens with post-send refresh ([1cd07ba](https://github.com/planetaryescape/mxr/commit/1cd07babc4721f7eaeb8e3e28c9ccac539b93f92))
* **tui:** polish command palette, send-confirm modal, diagnostics page, behaviors ([ef15d8c](https://github.com/planetaryescape/mxr/commit/ef15d8cfb2ca6c90c757d3c7cbbc880af9ad32be))
* **tui:** render safety verdict and override token in send-confirm modal ([faeb3b6](https://github.com/planetaryescape/mxr/commit/faeb3b62d548cef6d30c13bae3a5dd33a695efde))
* **tui:** rule form shell hooks + client-side validation ([8eba799](https://github.com/planetaryescape/mxr/commit/8eba7993329d2b6380e5cd8045ec63541db48424))
* **tui:** saved-search form data model + dispatch helpers ([dc7da63](https://github.com/planetaryescape/mxr/commit/dc7da6369f0307cf7327f50a598223bd92d71577))
* **tui:** saved-search modal + sidebar n/e/d + dispatch wiring ([1a59da4](https://github.com/planetaryescape/mxr/commit/1a59da4510afae12ed1a3fa721d2f975f503418a))
* **tui:** semantic controls in command palette ([049fba8](https://github.com/planetaryescape/mxr/commit/049fba8b61f23f4f0607515850a3752ce79a3123))
* **tui:** show Outlook device code during auth ([#33](https://github.com/planetaryescape/mxr/issues/33)) ([1ac9c12](https://github.com/planetaryescape/mxr/commit/1ac9c1211c98dbcd7422fa1a97d4ea17fb7b75a1))
* **tui:** task-oriented help section ([8108c21](https://github.com/planetaryescape/mxr/commit/8108c2119d78842085838bdc6bcea7bc2c570e4d))
* **tui:** undo binding — `u` reverses the most recent destructive mutation ([1a504fa](https://github.com/planetaryescape/mxr/commit/1a504fa95642e2c7cf9165ce238cbafaae8c2457))
* **tui:** whois modal + W keybind on focused sender ([ad09d4f](https://github.com/planetaryescape/mxr/commit/ad09d4f2ce2702ff16b111f5f7162ced2503a59a))
* **tui:** wire compose-flow safety check + override-token send-path ([594a408](https://github.com/planetaryescape/mxr/commit/594a4082981ad256a0a889248d625379bf323ac8))
* **tui:** wire dormant-thread hint + suggest row + expert palette entry ([e4c88a4](https://github.com/planetaryescape/mxr/commit/e4c88a466347b97eae58f5c5d50cc7063e8aee26))
* undo daemon pipeline — snapshot, restore, reverse-op ([40c7149](https://github.com/planetaryescape/mxr/commit/40c714961bf80a0b41df07144e3adfe650f3e2d2))
* undo log foundation — store layer + protocol surface ([d17dc21](https://github.com/planetaryescape/mxr/commit/d17dc215a78a57b97bfc48aa9c69bcc3abef38c1))
* v1 TUI ship-blockers — send-to-Sent visibility and IPC connection state ([c57c214](https://github.com/planetaryescape/mxr/commit/c57c21409c208c150e8a913465dc6739179f5dba))
* **web:** add activity log route ([e988b87](https://github.com/planetaryescape/mxr/commit/e988b8794de1b6cb172b497588e3facbb1e04836))
* **web:** dedicated keymap for full search + clearer palette selection ([ebe5371](https://github.com/planetaryescape/mxr/commit/ebe5371a9e9d2d65bbb0badd1dcf70467ed49b50))
* **web:** harden bridge stability ([ce5cc8d](https://github.com/planetaryescape/mxr/commit/ce5cc8d8d798a3d92535edd0feb3764db1ed37a3))
* **web:** improve mail navigation and sender profiles ([b103fe2](https://github.com/planetaryescape/mxr/commit/b103fe23ded7bc003fbd923d4f3cea7fcfee3e67))
* **web:** make analytics message rows open their thread ([43afcd5](https://github.com/planetaryescape/mxr/commit/43afcd5807df98cc4e7723ba203198e9fe57cc72))
* **web:** search keyboard flow — Enter hands off to the list, / refocuses query ([9ef3d13](https://github.com/planetaryescape/mxr/commit/9ef3d132e77a31c9f886c823b9d9e3c90ad2618b))
* **web:** split diagnostics into events/logs panels, expand settings, add tests ([007683c](https://github.com/planetaryescape/mxr/commit/007683cb3bcaabfefea18a615cdcc54148b70e6f))
* **web:** surface summaries and sender context ([295a184](https://github.com/planetaryescape/mxr/commit/295a184532e1f37cc76d7232fa2edf0c7d6ce10b))
* **web:** unify message/thread lists onto the mailbox list ([23093b0](https://github.com/planetaryescape/mxr/commit/23093b0b4aea2cede1395ea88cb9b39122fe2cd7))
* **whois:** query-time entity explainer with no new schema ([88507e5](https://github.com/planetaryescape/mxr/commit/88507e56afef8c64810de69916484bff8065234c))
* wire activity log end-to-end and add TUI modal ([1ce2552](https://github.com/planetaryescape/mxr/commit/1ce2552c1005ee31fd325bf35b0fa16004ddcbe6))


### Bug Fixes

* [] add desktop RPM license metadata ([373b61d](https://github.com/planetaryescape/mxr/commit/373b61d7f5ceedfb8bee380a0a8356cf8df565aa))
* [] add desktop RPM release metadata ([ddce3be](https://github.com/planetaryescape/mxr/commit/ddce3be52bd5bd5be492764dd6aa49c5752f2807))
* [] avoid envelope id decode panic ([4aaf902](https://github.com/planetaryescape/mxr/commit/4aaf9024539106e6dd5fd3ef62050bbbdefcdfc9))
* [] clean rebase fallout ([325e823](https://github.com/planetaryescape/mxr/commit/325e823cd1044a340f2489df37babf5962671ffe))
* [] clear ship-blocking test gates ([bb91a4c](https://github.com/planetaryescape/mxr/commit/bb91a4c7198dc1c9a9183cdcb9d54b63aa621a7a))
* [] harden desktop app shell ([f0bc7b0](https://github.com/planetaryescape/mxr/commit/f0bc7b07d46141c3d0ac5b30c9722061a3384936))
* [] harden desktop packaged smoke ([9afaf20](https://github.com/planetaryescape/mxr/commit/9afaf20087ff84baeb83d9f231ec81d8e1c783bd))
* [] reconcile folder mutations ([f09ff54](https://github.com/planetaryescape/mxr/commit/f09ff544ae5870204d068897e4682935277028a9))
* [] resolve label refs for mutations ([e5599fb](https://github.com/planetaryescape/mxr/commit/e5599fb1d90ace5d80a9653bcb5d8baada698a4a))
* [] route search escape to inbox ([b119307](https://github.com/planetaryescape/mxr/commit/b119307222ec7b900d97d2cd9331e5298b995e75))
* [] satisfy validation lint fixes ([41ea5e9](https://github.com/planetaryescape/mxr/commit/41ea5e93bba65fb8abe071cf3ec89ef5b2fa7184))
* [] satisfy warning-deny cargo check ([4dd90c0](https://github.com/planetaryescape/mxr/commit/4dd90c03fea99ed5d7093e23650da8eb9069a43b))
* [] scope sync command by account ([5622a69](https://github.com/planetaryescape/mxr/commit/5622a6964821cfc5c167ccdbbe10d90b8a76297d))
* [] tighten semantic search pipeline ([b5457be](https://github.com/planetaryescape/mxr/commit/b5457bed97e3f5483912bfb79db21082d7974a1d))
* align lockfile with v0.4.17 publish ([98ffc9e](https://github.com/planetaryescape/mxr/commit/98ffc9e48405bcfe17adb295bda2707a5341de7a))
* align web ipc protocol ([90a40a4](https://github.com/planetaryescape/mxr/commit/90a40a4a1f1d4ae825c21c5299141f38370a3709))
* analytics correctness — date floor, time-window filters, reply-pair backfill ([ce7270f](https://github.com/planetaryescape/mxr/commit/ce7270fb1955c9be78eb38e897b96e901ac7f173))
* avoid daemon restart on busy status ([4e2bca3](https://github.com/planetaryescape/mxr/commit/4e2bca318309e5aa583dada18588bf642bf32dd7))
* avoid interactive gmail keychain reads ([66e6e3c](https://github.com/planetaryescape/mxr/commit/66e6e3c042166e94af936e8a74e5ae34d15c27a2))
* avoid keychain prompts and recover message bodies ([61560bb](https://github.com/planetaryescape/mxr/commit/61560bbed8ed870e0f3b82590d65d49c0540ff92))
* build artifacts for release-prepare commits even when diff is version-only ([dc3ab44](https://github.com/planetaryescape/mxr/commit/dc3ab444ccf04bc1936e05b926fe5a235e55c571))
* cache browser-open files ([1a25dc9](https://github.com/planetaryescape/mxr/commit/1a25dc9020ecce5a648e87fe142ef5f75288c459))
* cache provider secrets and restore tui shortcuts ([bd4d8c6](https://github.com/planetaryescape/mxr/commit/bd4d8c66f3e12191f70196e95afd4334e7fb9400))
* **ci:** rustfmt, clippy, and openapi snapshot for the deliveries release ([2802d8d](https://github.com/planetaryescape/mxr/commit/2802d8df8ad92c53d8d32064480b2ae5f3c9225e))
* clarify message view states ([1dbac54](https://github.com/planetaryescape/mxr/commit/1dbac540599e977e87707690a557a39b40c5d633))
* **cli:** refresh and extend cli_help snapshots for AI-email subcommands ([202bd8e](https://github.com/planetaryescape/mxr/commit/202bd8e55bbc97fc15e314dfbdf4641ebe7ff691))
* collapse mxr into a single publishable package ([b83b9e6](https://github.com/planetaryescape/mxr/commit/b83b9e6bded098bc3a0e15a758ce91b7beac7370))
* compile web ui without built spa ([8a472a1](https://github.com/planetaryescape/mxr/commit/8a472a149679d533dd5c6ebdc9cd99194ef87d22))
* complete desktop mail parity ([b19ba5c](https://github.com/planetaryescape/mxr/commit/b19ba5c2553f6df3541505c17a398550432efcdb))
* **daemon:** acquire the index lock before touching the socket on startup ([bcf1de6](https://github.com/planetaryescape/mxr/commit/bcf1de6b976f30ad834f42676fce30093e2bfd03))
* **daemon:** truncate long attachment filenames ([#30](https://github.com/planetaryescape/mxr/issues/30)) ([bd2ef10](https://github.com/planetaryescape/mxr/commit/bd2ef10fa40d020900a8a8416659e21411f68c60))
* **deliveries:** detect order confirmations with dotted order numbers ([f7bde89](https://github.com/planetaryescape/mxr/commit/f7bde89cf6d5c7c9875aa48d35aef03297ff388e))
* **deliveries:** never detect spam or trashed mail as a delivery ([0f64557](https://github.com/planetaryescape/mxr/commit/0f64557ecc4b5d4f7c058827bd637e645ceb104a))
* detach autostarted daemon ([044f1d5](https://github.com/planetaryescape/mxr/commit/044f1d51846c383a34c0230e4fca226226b858b3))
* drop unused Timelike import to clear CI -D warnings ([67465a7](https://github.com/planetaryescape/mxr/commit/67465a72e5df30a33caf10e592100f4d7392c3ab))
* force SQLX_OFFLINE=true via build.rs in published crates ([425c383](https://github.com/planetaryescape/mxr/commit/425c383fbc34f8929e96ec3d177db7777050bec1))
* **gmail:** detect calendar invites delivered as .ics attachments ([b641692](https://github.com/planetaryescape/mxr/commit/b641692ae7e356c6b54378ad50df393d4c8d9175))
* gracefully handle missing providers instead of panicking ([5cbf473](https://github.com/planetaryescape/mxr/commit/5cbf4733cac2830732db4a448c487a05d27f0f5c))
* guard Command::Accounts dispatch with ensure_daemon_running ([ccb0cb9](https://github.com/planetaryescape/mxr/commit/ccb0cb93d52ba10a222e69fbc1b5745e8cd390b8))
* harden crates publish path ([d81a790](https://github.com/planetaryescape/mxr/commit/d81a7901f86715791270f82f4913869242a3a881))
* harden GA release path ([2c29820](https://github.com/planetaryescape/mxr/commit/2c298200d56e932934db88a82c20a23154a63db7))
* harden semantic local tests ([6449b65](https://github.com/planetaryescape/mxr/commit/6449b655734fde1837566da73f332cf09e84eb93))
* improve gmail account validation ([5238fb9](https://github.com/planetaryescape/mxr/commit/5238fb9174b57580814b4cf68a7ec69a72453c59))
* improve tui interaction recovery ([45d7fff](https://github.com/planetaryescape/mxr/commit/45d7fffbb68a28fb8e37a1b3ab452f1ec815a2ac))
* isolate account mutations ([c2426e9](https://github.com/planetaryescape/mxr/commit/c2426e91789bdc19d099457d6de7479988863992))
* keep cached mail visible without gmail auth ([e5e4193](https://github.com/planetaryescape/mxr/commit/e5e41939020c1e8c532c57e40fedd4f91ee50048))
* load accounts at startup for sidebar account switcher ([2142297](https://github.com/planetaryescape/mxr/commit/21422973fbae97a058de48d8abaa013c383ba244))
* make crates publish rerunnable ([7e63e79](https://github.com/planetaryescape/mxr/commit/7e63e791486b5fe71c906388cbe8bd8d96adae2a))
* make release-please handle workspace crates ([cc81ec9](https://github.com/planetaryescape/mxr/commit/cc81ec97d4ece363ba3317c5b187200391b09099))
* make search results behave like mailbox ([f183d1c](https://github.com/planetaryescape/mxr/commit/f183d1cd6a2d1a9f0aa9e9309d3eea54b91739e1))
* multi-account UX, sidebar jump, search multi-select ([4879bb0](https://github.com/planetaryescape/mxr/commit/4879bb00bfff7841bc62ee77b0eb7a2955708de5))
* package sqlx metadata with mxr-store ([e975409](https://github.com/planetaryescape/mxr/commit/e9754094857a49e0bdb97729722c121d36a673fb))
* pass Gmail OAuth secrets to release build steps ([e977035](https://github.com/planetaryescape/mxr/commit/e977035837972b62300066a70a91f493f3a08955))
* per-crate .sqlx and conditional build.rs offline default ([ea43325](https://github.com/planetaryescape/mxr/commit/ea433258be28d9ccd1dda8cbf10871457f91c36b))
* polish desktop workflows ([c08629d](https://github.com/planetaryescape/mxr/commit/c08629d8178d9f1249bcfdedc12d9428a4053d20))
* prevent silent label corruption with transactional set_message_labels ([0a6d40e](https://github.com/planetaryescape/mxr/commit/0a6d40e8a05d4f2ab2bb56e3473d0dcd6e613756))
* prioritize file attachments in attachment modal ([486b062](https://github.com/planetaryescape/mxr/commit/486b0621145ca1622b1f1c3ba0ac92e4da87319b))
* **provider-gmail:** normalize OAuth scope order in keychain lookups ([8844df4](https://github.com/planetaryescape/mxr/commit/8844df4d18a859d2c3663600ab010af1d0077993))
* publish mxr-test-support before its dependents ([d107405](https://github.com/planetaryescape/mxr/commit/d10740514f01b28242582ca179e2da393a89497b))
* publish test support before readers ([81f8714](https://github.com/planetaryescape/mxr/commit/81f8714ffc9860b20497869809217565f1178a9b))
* publish vendored async-imap before mxr release ([8fd5a77](https://github.com/planetaryescape/mxr/commit/8fd5a777e96bdb5e27955b1282a2ed9ea1db1962))
* recover broken daemon sockets ([4d5f298](https://github.com/planetaryescape/mxr/commit/4d5f298cc1562765adef4eaf20a90ac2a2519722))
* rehydrate missing bodies and render non-text mail ([a744ff7](https://github.com/planetaryescape/mxr/commit/a744ff7fc1e224a42d2fad368e58161201950f65))
* release 0.4.10 search tab hydration ([b0b4354](https://github.com/planetaryescape/mxr/commit/b0b4354f093e21764b0d76a543995afe55f12762))
* release 0.4.12 tui contrast recovery ([1dd0ed6](https://github.com/planetaryescape/mxr/commit/1dd0ed6a2049124193634433a7d4e2f4d2250af5))
* release 0.4.13 search recovery ([ccd72f9](https://github.com/planetaryescape/mxr/commit/ccd72f9f00578fb9fba61237fd053c146d9d0d6a))
* release 0.4.14 publish mxr-web ([33c9a39](https://github.com/planetaryescape/mxr/commit/33c9a39fd3b0c84d3d82be06be4728412805a4d9))
* release 0.4.5 daemon startup recovery ([fd3a272](https://github.com/planetaryescape/mxr/commit/fd3a27228a88feff778fd037630fe4ba05d30e8b))
* release 0.4.6 gmail stale cursor recovery ([13a11cd](https://github.com/planetaryescape/mxr/commit/13a11cdb9880c991a5c50bf057a240ac520277bd))
* release 0.4.7 mailbox polish ([a4b9fe0](https://github.com/planetaryescape/mxr/commit/a4b9fe0cb272f41b1a92f506765a640086125a1f))
* release 0.4.8 publish sync ([5b84d25](https://github.com/planetaryescape/mxr/commit/5b84d255a6c877a12833bb7563abb9a295136979))
* release 0.4.9 search and diagnostics recovery ([961a55f](https://github.com/planetaryescape/mxr/commit/961a55fdaa8548a2d665771bc0cd08202cb4ec34))
* release v0.4.19 workflow outputs ([5481009](https://github.com/planetaryescape/mxr/commit/5481009a5fda065dba74fc48c13f0bc999c90993))
* release v0.4.20 artifact scope ([67a40a4](https://github.com/planetaryescape/mxr/commit/67a40a44535f2b9b8df53b890f591cdd8020a5f4))
* release v0.4.21 cli scope ([7d65512](https://github.com/planetaryescape/mxr/commit/7d655124f1f40ab976bc6db4582f41672d9b7446))
* release v0.4.22 publish gate ([b8ba725](https://github.com/planetaryescape/mxr/commit/b8ba725b3eb3376a810f17af0c672c29caa4fb22))
* remove production panic and unwrap paths ([6b2e11a](https://github.com/planetaryescape/mxr/commit/6b2e11a3da56a5aac2158ff26e995c92716d46b1))
* remove redundant crates wait ([a50e384](https://github.com/planetaryescape/mxr/commit/a50e384663ce26db6c4e348c8de6719fdf176213))
* render complete homebrew formulas ([7984e91](https://github.com/planetaryescape/mxr/commit/7984e912f49c859cb8ebc9237e86af3afbf7a968))
* render onboarding modal globally and surface sync errors ([872f36c](https://github.com/planetaryescape/mxr/commit/872f36ce921bc107658cbc82bf5dccd8f8dd97ba))
* reopen healthy daemon web bridge ([f83c83d](https://github.com/planetaryescape/mxr/commit/f83c83d426c295fdc618c5b416b24b328ee1aa81))
* repair 0.4.1 release ([991553e](https://github.com/planetaryescape/mxr/commit/991553eeeb42ea2ad3d6624c5a5da15636ffd930))
* repair 0.4.2 release ([59d8b7a](https://github.com/planetaryescape/mxr/commit/59d8b7a55bd1b25f828aa2e4dcd6eee531790392))
* repair GA release checks ([5dca5a8](https://github.com/planetaryescape/mxr/commit/5dca5a813abd265f3731dc3a990f2642b4d40ba0))
* replace macos keychain items during repair ([b7cf41d](https://github.com/planetaryescape/mxr/commit/b7cf41de53b72ca4ac3f37e0e905ced28fe665f1))
* resolve gmail oauth flow client-side so desktop uses loopback ([ccb6c58](https://github.com/planetaryescape/mxr/commit/ccb6c584d7c3d2a35a922ea7f53933a6c28e0f32))
* restore browser open flow ([b36fdd2](https://github.com/planetaryescape/mxr/commit/b36fdd27c3da1ecde4d71c8c1629f25a8acdb823))
* restore release CI checks ([8f60708](https://github.com/planetaryescape/mxr/commit/8f6070876d411220a33c555de79499fb324bd741))
* restore release version sync ([aa75cc5](https://github.com/planetaryescape/mxr/commit/aa75cc5bf94726a07adf69d14ddce5b694af6135))
* restore semantic release checks ([cd30269](https://github.com/planetaryescape/mxr/commit/cd3026912be48993cf3a21ea5a23241f5830ef4a))
* retry crates publish on rate limits ([2f56d1c](https://github.com/planetaryescape/mxr/commit/2f56d1c802fac205e949b92260b5682c3d7cfcd0))
* sanitize IMAP protocol parse errors for end users ([fe85eea](https://github.com/planetaryescape/mxr/commit/fe85eea5d6995104eb30fe46feba826fb3d1c21d))
* satisfy current stable clippy ([97cb021](https://github.com/planetaryescape/mxr/commit/97cb02187b35fe1c9de6aaec577b42407a214e5a))
* satisfy rust 1.95 clippy ([41f7f97](https://github.com/planetaryescape/mxr/commit/41f7f9751df372a9946a41f6cfb0e2e3072b8f94))
* satisfy rust 1.95 clippy again ([399ca6d](https://github.com/planetaryescape/mxr/commit/399ca6d615527873b5943119298c9fd5a2b0c3b5))
* **snapshots:** refresh openapi spec summary for AI-email schemas ([6437cd0](https://github.com/planetaryescape/mxr/commit/6437cd06e895f777429ef6bc9ffff36bf70c2a6a))
* stabilize daemon startup ([385c7c3](https://github.com/planetaryescape/mxr/commit/385c7c3d1380f772577d6fe013b49169674f1067))
* stabilize release test fixtures ([7bd9dae](https://github.com/planetaryescape/mxr/commit/7bd9dae69ee4418e373ca1d548391fa12a7d13f9))
* stabilize tui archive selection and search results ([3d272ba](https://github.com/planetaryescape/mxr/commit/3d272ba1465c040bf2820f2534a4d11876789523))
* stabilize TUI body and mutation state ([751f56f](https://github.com/planetaryescape/mxr/commit/751f56fc29f6914167513cee51191ce7ba3bae35))
* stop background workers from starving the SQLite connection pool ([ad6b42b](https://github.com/planetaryescape/mxr/commit/ad6b42be536dd90057227fd9c048cbf4936fb102))
* **store:** make migration 026 idempotent for pre-versioning backfill ([417676d](https://github.com/planetaryescape/mxr/commit/417676d8cdb5455cc1b8f073d153d8a078f51c76))
* support draft-first compose flow ([44a4013](https://github.com/planetaryescape/mxr/commit/44a40131ed6ac434195f57069503b98ed4e6d8f0))
* surface web auth recovery ([99fc2ac](https://github.com/planetaryescape/mxr/commit/99fc2ac9a71ff0468c2a5504ddb8ea455ee48bec))
* sync Cargo.lock to 0.4.56 workspace version ([3e10d7f](https://github.com/planetaryescape/mxr/commit/3e10d7f83cc3017798774da278752b50bb2c21e9))
* tolerate duplicate crate publishes ([3c6a37e](https://github.com/planetaryescape/mxr/commit/3c6a37ebfddc8410301fb3994d041b628c2c3834))
* topologically sort crates.io publish loop ([0c85af7](https://github.com/planetaryescape/mxr/commit/0c85af76e9605ae04839dd172305c723ed74781f))
* track daemon background task lifecycle ([a2b7d4e](https://github.com/planetaryescape/mxr/commit/a2b7d4e64103bde1581217d08f06acd1b09245fa))
* **tui:** clean up deprecated method and dead code warnings ([5f21103](https://github.com/planetaryescape/mxr/commit/5f21103da80758fc6187abb6972e95aea200ef17))
* **tui:** open a delivery's email inline in a split preview ([6b9f8e7](https://github.com/planetaryescape/mxr/commit/6b9f8e72beef02979f618fe6a5a1af157550afb3))
* **tui:** prevent archive bounce-back and polish UI surfaces ([6a9dd86](https://github.com/planetaryescape/mxr/commit/6a9dd86a441ab35f4d0a14c81f32e171313da021))
* **tui:** restore line numbers in mail list ([bebb3eb](https://github.com/planetaryescape/mxr/commit/bebb3ebf90d8ef956eca7e1f4a834281d3555673))
* **tui:** unblock the Deliveries screen and enrich its rows ([90dfa0b](https://github.com/planetaryescape/mxr/commit/90dfa0bf823f297cf2e63f058a74e5a9b336641e))
* **tui:** unstick the y summarize keybinding and broaden auto-summary ([8c0e170](https://github.com/planetaryescape/mxr/commit/8c0e170f40d1d8186328e19635aa672ade3d2c22))
* unblock crates publish ([491f068](https://github.com/planetaryescape/mxr/commit/491f06834bcb2136a3bc1ee18f7276460519d0b4))
* unblock crates publish for 0.4.4 ([78dd8d8](https://github.com/planetaryescape/mxr/commit/78dd8d81b28cfb60e8e62ae28c48e37c49ac4173))
* unify search message view behavior ([5c4bf60](https://github.com/planetaryescape/mxr/commit/5c4bf6020f5725aa469f375fc9d80cc580c11ba3))
* update rustls webpki advisory ([a3f3dbd](https://github.com/planetaryescape/mxr/commit/a3f3dbd3a2ab30a72e0b7e04ba9ba9f0a20003af))
* upsert labels and envelopes by natural key to survive ID derivation change ([3d4414b](https://github.com/planetaryescape/mxr/commit/3d4414b2da247fa6533a865e04a2300331fd95bd))
* wait for crates index propagation ([f3bc0bc](https://github.com/planetaryescape/mxr/commit/f3bc0bcf65501031ea56c37350057ec9a40b1508))
* **web:** clear stale bridge-port + surface child failures ([9acee8c](https://github.com/planetaryescape/mxr/commit/9acee8c87bcb6ae71d93df05a0109c187279e127))
* **web:** use loopback OAuth flow for Gmail onboarding, not device-code ([a9c80ab](https://github.com/planetaryescape/mxr/commit/a9c80ab659f333c60262e92dc2e0e03a0fc7cf10))


### Performance

* [] prewarm reply context on message open for instant r/a ([f0f4209](https://github.com/planetaryescape/mxr/commit/f0f4209ed174bd618a86d776f17caa209651440f))
* **daemon:** unblock sync hot path and split IPC priority lanes ([fd6ee5f](https://github.com/planetaryescape/mxr/commit/fd6ee5fb39bc2abeb6086676abe776e6e0d850a3))
* **tui:** de-quadratic the mail-list row markers ([0b2beef](https://github.com/planetaryescape/mxr/commit/0b2beef9e130acac8824bc2e455bd480d6b805c2))


### Refactoring

* [] compose TUI app state ([e616c31](https://github.com/planetaryescape/mxr/commit/e616c31ba4e6ce7879c60da95567472864b908b7))
* [] formalize IPC buckets ([ed16b37](https://github.com/planetaryescape/mxr/commit/ed16b372f6bdf9f8948982d7c62f51bf89911e42))
* [] make workspace boundaries real ([09fba61](https://github.com/planetaryescape/mxr/commit/09fba6146d4bdf04b0ea433cc6faab0d7907c635))
* [] migrate command palette to action registry ([65adfc9](https://github.com/planetaryescape/mxr/commit/65adfc987dfc11ae2b1c8f3df9cbc56cb591d36c))
* [] migrate global keymap to action registry ([9c7d9e6](https://github.com/planetaryescape/mxr/commit/9c7d9e6eb9e03aa2adf4e04e0878f5cb637e9ab4))
* [] migrate HelpDialog and StatusBar to action registry ([0f46da7](https://github.com/planetaryescape/mxr/commit/0f46da77cb46bf364a78c890318b58bd06801e29))
* [] remove native desktop app ([261e5f5](https://github.com/planetaryescape/mxr/commit/261e5f59937af7bc78205931037d638f865b16a9))
* [] rename client shell route ([13967c0](https://github.com/planetaryescape/mxr/commit/13967c0aa1ae69980719247bbd00afb1be63060d))
* add Role enum + Label.role field for MSP §2.3 alignment ([aa29c46](https://github.com/planetaryescape/mxr/commit/aa29c46dfce6ec8cebdacaa116b80401b3f1f36b))
* add typed SyncCursorExpired error variant ([b31d006](https://github.com/planetaryescape/mxr/commit/b31d00637ab7017019e84ab9d948a56e5d1eafb9))
* drop dead RecoveringNotFoundProvider mock ([b483d0c](https://github.com/planetaryescape/mxr/commit/b483d0c932573b1631f1f16de224b7b0aa96c3ec))
* finish oversized integration module split ([d60b8c9](https://github.com/planetaryescape/mxr/commit/d60b8c95f5643314472447bc0ff6dd053f5934a1))
* finish rust idiom cleanup tail ([f40c896](https://github.com/planetaryescape/mxr/commit/f40c8965d152089838de851a1ca0efa3eb7a21da))
* fix large enum variants and API convention lints ([2d8d183](https://github.com/planetaryescape/mxr/commit/2d8d183afbce07b5b77da2a0b966dd01a0243d60))
* harden async runtime paths ([0147e4f](https://github.com/planetaryescape/mxr/commit/0147e4f1fd838a483b1fb8c2ebd293bc7e264545))
* has_more on SyncBatch (MSP Phase C) ([8b668f2](https://github.com/planetaryescape/mxr/commit/8b668f29a6b8d225636f0ff02ce3d90ba57b08ff))
* idiomatic Rust pass and safe simplifications ([e332700](https://github.com/planetaryescape/mxr/commit/e332700b17fcd4d6936ba8969b280fac631dc5ef))
* improve smart pointer usage ([7523f23](https://github.com/planetaryescape/mxr/commit/7523f2322bce2c491f9143735f0c13ac72e5a310))
* introduce parameter structs for wide helper APIs ([a517153](https://github.com/planetaryescape/mxr/commit/a51715384b8dcee3e11373d226cf2f9204046dca))
* namespace SyncCapabilities into sync/mutate/search/push ([37b771f](https://github.com/planetaryescape/mxr/commit/37b771f66e48f6d16ac064b7b12e974c7e795053))
* opaque SyncCursor (MSP Phase B) ([0588142](https://github.com/planetaryescape/mxr/commit/05881425e4fcfdc691f222729398cbf1c02357ee))
* remove remaining API lint allowances ([c84f128](https://github.com/planetaryescape/mxr/commit/c84f128a19986c63e4e3695daa43efd56420905e))
* split oversized integration modules ([dd37ddf](https://github.com/planetaryescape/mxr/commit/dd37ddf10f766aeb0360bc89c6710cd70cf9d8c3))
* **test-support:** promote daemon-spawning helpers from cli_journey ([6c7e2ea](https://github.com/planetaryescape/mxr/commit/6c7e2ea4298629583c516502875f651ec499301b))
* track applied DB migrations in schema_migrations table ([98f6a87](https://github.com/planetaryescape/mxr/commit/98f6a8753956bf77ed95dd01ee5cb2484c7bcb5a))
* typed HandlerError for daemon IPC handlers ([1729ba4](https://github.com/planetaryescape/mxr/commit/1729ba4e213456d2a22a8307c119218663048ee6))
* typed MxrError::RateLimited and IpcErrorKind::RateLimited ([6efddc0](https://github.com/planetaryescape/mxr/commit/6efddc007c421e181306156361fed2875a49f27f))
* unified apply_mutation + idempotent retry (MSP Phase D) ([6ccd3a8](https://github.com/planetaryescape/mxr/commit/6ccd3a81c388922e2762fc9e7f7b6f124b35b9b2))


### Documentation

* [] add user activity log design plan ([f653dfd](https://github.com/planetaryescape/mxr/commit/f653dfde4c9002fe7603c88c82c347ec5b9a63c6))
* [] add web parity-closure plan ([90ef1d9](https://github.com/planetaryescape/mxr/commit/90ef1d9f214512e81e06a3cced68068ddf7d0d3c))
* [] align IPC docs ([0853eef](https://github.com/planetaryescape/mxr/commit/0853eefc5f19074728fb1af977d416ecff50e36a))
* [] document provider truth seams ([669a881](https://github.com/planetaryescape/mxr/commit/669a881845be48c1551c5211e68425b6b464e535))
* [] document sticky demo mode, has:link filters, and link indicator ([a977734](https://github.com/planetaryescape/mxr/commit/a9777343c3dc2fa28cbd2250f2350e2517efaab1))
* [] document sync and search lifecycle ([1ec205f](https://github.com/planetaryescape/mxr/commit/1ec205fe3cbcf9f717b9069eb979dd276b349ede))
* [] document web parity closure on docs site ([53eab0f](https://github.com/planetaryescape/mxr/commit/53eab0f5ee44be59b66ba618c54be3ae4310d97b))
* [] explain no native desktop app ([da61537](https://github.com/planetaryescape/mxr/commit/da61537ae33ff5ae46ada9b45b282565813172d5))
* [] rewrite docs landing copy ([83b5436](https://github.com/planetaryescape/mxr/commit/83b5436caa5f571d64680e41c1c0cbd6c023a605))
* [] update semantic search docs ([33b38b0](https://github.com/planetaryescape/mxr/commit/33b38b0ef01d4db64302332bcfce75b745239b06))
* [] update site architecture ([3599ff2](https://github.com/planetaryescape/mxr/commit/3599ff2084683a54ac1c117150b0593eb96269f1))
* add A010 — CLI v1 ship gate addendum ([ad45e17](https://github.com/planetaryescape/mxr/commit/ad45e1723bab43f0f5db37632fe6ec33d288082e))
* add analytics guide + CLI reference section ([c1ee7c5](https://github.com/planetaryescape/mxr/commit/c1ee7c58c6dd64133e308401dbb3b917ec53f85b))
* add analytics workflow recipes — situations, not commands ([d189ead](https://github.com/planetaryescape/mxr/commit/d189ead30ec9c3e430d10c98bbf643de82e55c39))
* add idiomatic Rust rubric ([580c431](https://github.com/planetaryescape/mxr/commit/580c431f8bc0334ca84de968930b9d529abfa588))
* add implementation journey context ([c222715](https://github.com/planetaryescape/mxr/commit/c2227153abed56a76b87b2acf73053f8460ce3e3))
* agent build-and-verify workflow in AGENTS.md ([9157cab](https://github.com/planetaryescape/mxr/commit/9157cab1298eab0941795d1546c543c048ad5496))
* align docs and harden site build ([257032b](https://github.com/planetaryescape/mxr/commit/257032bbcea859180005b71b47a10207f33352d5))
* archive jwz-threading audit to extractable-crates/done/ ([2fcffc3](https://github.com/planetaryescape/mxr/commit/2fcffc3496c360f1fcf67f19ec2537e9ffb47a00))
* **bridge:** add v0.5 HTTP bridge guide ([ddb9273](https://github.com/planetaryescape/mxr/commit/ddb92732f442c7f5dc1cdd0cf3560fcb85bb4b33))
* capture mail-threading extraction lessons ([f46af68](https://github.com/planetaryescape/mxr/commit/f46af68128000c716eeadeb2a4bd4d13d25dc915))
* capture naming lesson; mail-threading v0.1.1 description fix ([2dc9431](https://github.com/planetaryescape/mxr/commit/2dc94310e6bb96c9d7b5dda9ff9f333b9fec2043))
* **cli:** document undo, snooze, accounts repair/disable/remove, search operators ([c1a1e13](https://github.com/planetaryescape/mxr/commit/c1a1e1331f7c4229320bba452f7e5952cc2a71e9))
* consolidate MSP initiative under docs/msp/ with README + ROADMAP ([ffc2c69](https://github.com/planetaryescape/mxr/commit/ffc2c69e8fe037eab3fe5e76eccf02fa7ee91a9c))
* consolidate per-feature design docs into single files ([efaeb9a](https://github.com/planetaryescape/mxr/commit/efaeb9acf3dec54adf3c93663552fa6eff97f3a8))
* cover Analytics tab + self-healing rebuild flow ([99266af](https://github.com/planetaryescape/mxr/commit/99266af1517826fa4a4346f8f8b80075a42579ed))
* cover mxr wrapped + storage --by message drill-down ([8c93236](https://github.com/planetaryescape/mxr/commit/8c932364f14bcd3f1567ad72313ca3feb76be10b))
* **deliveries:** document the deliveries surface; fix analytics keybinding ([be2daa5](https://github.com/planetaryescape/mxr/commit/be2daa5eb6a5e0bdc76b49775c03888a8f8f237a))
* document account selection ([b2d6767](https://github.com/planetaryescape/mxr/commit/b2d67675202eed588c376fea59eb160a813f85d1))
* document semantic relationship workflows ([cfbf377](https://github.com/planetaryescape/mxr/commit/cfbf37778ef11aa8a423df285fc5a817a8b572a7))
* document the calendar invites page across clients ([d15d2bd](https://github.com/planetaryescape/mxr/commit/d15d2bd6577da7b6507e1d2619de523620f5f5db))
* drop ghost flags, lead with mxr accounts add for IMAP/SMTP ([58ea627](https://github.com/planetaryescape/mxr/commit/58ea627e007bc3f8465e08dcb89359aed9d49213))
* expand Gmail setup guide with full walkthrough ([b2e7c44](https://github.com/planetaryescape/mxr/commit/b2e7c4499b18840cf4564b3b282980a81817b4bf))
* fix install instructions to match shipped reality ([1d4fbc9](https://github.com/planetaryescape/mxr/commit/1d4fbc9e8973559f115cd7b013f8685d9022213b))
* flagship polish on landing page (impeccable pass) ([5b5eda1](https://github.com/planetaryescape/mxr/commit/5b5eda1db95739cefa55aca9c64b4b01c4d9fe12))
* install path, quick-start, troubleshooting, release-please drift ([b5e982d](https://github.com/planetaryescape/mxr/commit/b5e982d305970a0c6931a31a0e304bba1f56abf3))
* Mail Sync Protocol (MSP) spike — spec, alignment audit, blog draft ([ccdef99](https://github.com/planetaryescape/mxr/commit/ccdef992c9b9dd7721e724f3139159c4c2c15e7f))
* mark list-unsubscribe extraction complete ([c1c2bea](https://github.com/planetaryescape/mxr/commit/c1c2bea168428607f2cd13a129c4bf037c82ad03))
* mark mail-query extraction complete ([fd3d55f](https://github.com/planetaryescape/mxr/commit/fd3d55f50eb0019366789677bef9088e9137f3e4))
* mark mail-threading extraction complete ([6fcd00b](https://github.com/planetaryescape/mxr/commit/6fcd00b264ba5644d517f30d26ea90f2012c95e4))
* mark mailbox-formats extraction complete ([af0c5a7](https://github.com/planetaryescape/mxr/commit/af0c5a71a40fdd5b1122329cb61669b9bd96955e))
* mark MSP Phase A done in ROADMAP ([a39c84c](https://github.com/planetaryescape/mxr/commit/a39c84c02aa9bcc303963b9a145507d95afd467a))
* mark MSP Phase B done in ROADMAP ([1aaf2c9](https://github.com/planetaryescape/mxr/commit/1aaf2c9d055091e6fbf2b29ba1cc8b9ef0a0564d))
* move ai-email vision out of vision subfolder ([c38c3f7](https://github.com/planetaryescape/mxr/commit/c38c3f7bc23b2aab8c19e3bf38d1752add780a50))
* move compose/humanizer/llm/keychain into wont-do/ ([0ef5fa6](https://github.com/planetaryescape/mxr/commit/0ef5fa68042a81850d769bda08e938a0d4ec3f38))
* overhaul landing page (dark default, no cards, nyx-influenced) ([86d738b](https://github.com/planetaryescape/mxr/commit/86d738b820d6126768cf1a18ae1b80154f19f07b))
* raise the publishing bar; mark 04/06/08/09 as won't-do ([c583695](https://github.com/planetaryescape/mxr/commit/c5836955d946389f3032ad495f6356f5bcaf34be))
* refresh architecture boundary model ([f30a550](https://github.com/planetaryescape/mxr/commit/f30a550030ff96d418f33e35511da041e89bae0e))
* refresh mxr guidance and references ([471c01d](https://github.com/planetaryescape/mxr/commit/471c01d4983b36a37ea67c7f1d229553ff4a663b))
* refresh vision handoff ([2e96231](https://github.com/planetaryescape/mxr/commit/2e96231d6069a32f56002203800e290a61a54d8e))
* restore docs navigation on the landing page ([b4417e1](https://github.com/planetaryescape/mxr/commit/b4417e1d61af0a7599fcd9e95c7b92f4d22650b6))
* restructure landing page with proper layering and lineage timeline ([1e5aac0](https://github.com/planetaryescape/mxr/commit/1e5aac0287d4aafca23c2949cfe8259dfee958d7))
* retract MSP Phase G; body delivery is now a negotiated capability ([0c36455](https://github.com/planetaryescape/mxr/commit/0c36455002e40d84d343a64ed7170ef5f623f052))
* rewrite landing page around user superpowers ([8ca481a](https://github.com/planetaryescape/mxr/commit/8ca481ae23090f3553b7b92104fe029704f02956))
* **site:** add user-facing guides for activity log and AI features ([7be263e](https://github.com/planetaryescape/mxr/commit/7be263e42bf3de4c6fdfe5db352d92bd5a08d29b))
* spec §2.5 + alignment audit §2.5 + ROADMAP changelog updated. ([13ea42e](https://github.com/planetaryescape/mxr/commit/13ea42e8c747ea2c903bc7c31daaf23afad83774))
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
