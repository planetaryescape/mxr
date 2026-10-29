# Changelog

## [0.6.193](https://github.com/planetaryescape/mxr/compare/v0.6.192...v0.6.193) (2026-10-29)


### Bug Fixes

* quote the integration test filters so the release workflow parses again ([#811](https://github.com/planetaryescape/mxr/issues/811)) ([5776fec](https://github.com/planetaryescape/mxr/commit/5776fec8bc09a186eb3bc2404861ab28e6103694))

## [0.6.192](https://github.com/planetaryescape/mxr/compare/v0.6.191...v0.6.192) (2026-10-29)


### Bug Fixes

* leaving a conversation saves the reply as a Draft on its Messages row, in every client ([#808](https://github.com/planetaryescape/mxr/issues/808)) ([2c33f44](https://github.com/planetaryescape/mxr/commit/2c33f44309a42037e236987158c68f8c728b980d))

## [0.6.191](https://github.com/planetaryescape/mxr/compare/v0.6.190...v0.6.191) (2026-10-29)


### Bug Fixes

* let expired key-move notices stop instead of blocking releases ([#805](https://github.com/planetaryescape/mxr/issues/805)) ([24cd0a0](https://github.com/planetaryescape/mxr/commit/24cd0a034dbe04ed85c4d4ea77ba4159ac2bfac6))


### Documentation

* take mxr worktrees from the warm pool ([#804](https://github.com/planetaryescape/mxr/issues/804)) ([b226b04](https://github.com/planetaryescape/mxr/commit/b226b04cf56caaccc6956e2e53c0a618dfddd049))

## [0.6.190](https://github.com/planetaryescape/mxr/compare/v0.6.189...v0.6.190) (2026-10-29)


### Bug Fixes

* keep a to-do when a rescan of the same email moves its due date to another month ([#802](https://github.com/planetaryescape/mxr/issues/802)) ([f062799](https://github.com/planetaryescape/mxr/commit/f0627997e1210a5786584639f0d270cc5534ab5b))

## [0.6.189](https://github.com/planetaryescape/mxr/compare/v0.6.188...v0.6.189) (2026-10-29)


### Bug Fixes

* keep keyboard shortcuts working after a click inside an HTML mail ([#797](https://github.com/planetaryescape/mxr/issues/797)) ([7f8e8f3](https://github.com/planetaryescape/mxr/commit/7f8e8f3f9420e476381a0f0888b2243d68e78614))

## [0.6.188](https://github.com/planetaryescape/mxr/compare/v0.6.187...v0.6.188) (2026-10-29)


### Bug Fixes

* reopen the right conversation when undo brings a person back in Messages ([#794](https://github.com/planetaryescape/mxr/issues/794)) ([b36d8d3](https://github.com/planetaryescape/mxr/commit/b36d8d33e0c7e9749e74d42d34d92cc0f18afaa8))

## [0.6.187](https://github.com/planetaryescape/mxr/compare/v0.6.186...v0.6.187) (2026-10-29)


### Bug Fixes

* list every account's labels and subscriptions under All accounts ([#790](https://github.com/planetaryescape/mxr/issues/790)) ([38f2fd6](https://github.com/planetaryescape/mxr/commit/38f2fd64c3003ddab9c154ce5f0a7d0486cd50df))

## [0.6.186](https://github.com/planetaryescape/mxr/compare/v0.6.185...v0.6.186) (2026-10-29)


### Bug Fixes

* a browser unsubscribe is a handoff, not a confirmed send ([#788](https://github.com/planetaryescape/mxr/issues/788)) ([28f1f92](https://github.com/planetaryescape/mxr/commit/28f1f92f56cc577828e038912a23b288917a9b6e))

## [0.6.185](https://github.com/planetaryescape/mxr/compare/v0.6.184...v0.6.185) (2026-10-29)


### Bug Fixes

* clear the seroval advisory and pin the clock in the bill re-date to-do test ([#782](https://github.com/planetaryescape/mxr/issues/782)) ([f4b1e2c](https://github.com/planetaryescape/mxr/commit/f4b1e2c6cd02089bde977d74674a57a405260b42))
* keep daemon auth kinds through every CLI command ([#787](https://github.com/planetaryescape/mxr/issues/787)) ([3950d42](https://github.com/planetaryescape/mxr/commit/3950d4243d6861bee35bde69351a6a5e12a7e2fb))
* mark account rows in assistant, Owed and Place, and read unknown colour slots ([#785](https://github.com/planetaryescape/mxr/issues/785)) ([22f9624](https://github.com/planetaryescape/mxr/commit/22f9624999be235b5a087cc0b6f6687b85443c62))
* retire the key moves announced until 0.6.185 ([#784](https://github.com/planetaryescape/mxr/issues/784)) ([be8dccf](https://github.com/planetaryescape/mxr/commit/be8dccf6ef09333997ef6eb1810f67d1bb499420))
* stop idle daemons that have no real mail account ([#781](https://github.com/planetaryescape/mxr/issues/781)) ([166ccc2](https://github.com/planetaryescape/mxr/commit/166ccc26186d2b09e204b81e922c25d61589c369))


### Performance

* make the slowest Rust tests fast without losing any check ([#786](https://github.com/planetaryescape/mxr/issues/786)) ([8278a73](https://github.com/planetaryescape/mxr/commit/8278a7314f04228b2a80aa363972d7ecac96c34a))

## [0.6.184](https://github.com/planetaryescape/mxr/compare/v0.6.183...v0.6.184) (2026-10-29)


### Bug Fixes

* show the & in desktop menu titles such as Focus & reply ([#777](https://github.com/planetaryescape/mxr/issues/777)) ([f607e6c](https://github.com/planetaryescape/mxr/commit/f607e6c9aa6ed8353d17fd58346b7a268d824ddd))


### Documentation

* amend D169 so the desktop menu gives shortcuts only to Mod-chord actions ([c382641](https://github.com/planetaryescape/mxr/commit/c38264191ce66398795c1490bcce6ce0e13940f8))
* record the App Shortcuts rebind check for the desktop menu ([928fa34](https://github.com/planetaryescape/mxr/commit/928fa3435c37fe6778ab94ef0e9fa28bd3e25aeb))
* tell agents how Cargo target dirs get swept ([#775](https://github.com/planetaryescape/mxr/issues/775)) ([9c94b34](https://github.com/planetaryescape/mxr/commit/9c94b34803209a9b6432727da83f1ca8ab6b19ce))

## [0.6.183](https://github.com/planetaryescape/mxr/compare/v0.6.182...v0.6.183) (2026-10-28)


### Features

* fold messages and quotes and archive-and-step in the TUI reader; web dialog keys from the definition ([#762](https://github.com/planetaryescape/mxr/issues/762)) ([78df13d](https://github.com/planetaryescape/mxr/commit/78df13d8855a2b4afe983cd0e673c1833be7d9ff))


### Bug Fixes

* let upsert_envelope wait for another writer instead of failing busy ([#769](https://github.com/planetaryescape/mxr/issues/769)) ([e3c036b](https://github.com/planetaryescape/mxr/commit/e3c036b6cf9d4e4c34c183c1110c375ff797ffbf))

## [0.6.182](https://github.com/planetaryescape/mxr/compare/v0.6.181...v0.6.182) (2026-10-28)


### Bug Fixes

* index the text the reader shows, not raw HTML ([#771](https://github.com/planetaryescape/mxr/issues/771)) ([2a70c2f](https://github.com/planetaryescape/mxr/commit/2a70c2f9ea35801c39892ac2e72a5fcfe2efc74d))
* retire the key moves announced until 0.6.180 ([#770](https://github.com/planetaryescape/mxr/issues/770)) ([cc4acbe](https://github.com/planetaryescape/mxr/commit/cc4acbe6d5c1de6e0855e65b0b29ac18cbcc3b7b))

## [0.6.181](https://github.com/planetaryescape/mxr/compare/v0.6.180...v0.6.181) (2026-10-28)


### Bug Fixes

* keep malformed email HTML from crashing the reader ([#767](https://github.com/planetaryescape/mxr/issues/767)) ([4fe5d04](https://github.com/planetaryescape/mxr/commit/4fe5d04eeafb9e49bc65099ad606045e5cbc2e12))
* take the newest 1,000 matches for --search bulk commands and say when more match ([#765](https://github.com/planetaryescape/mxr/issues/765)) ([6627f32](https://github.com/planetaryescape/mxr/commit/6627f32cf8699faf36cab015aab86d291d5664c5))

## [0.6.180](https://github.com/planetaryescape/mxr/compare/v0.6.179...v0.6.180) (2026-10-28)


### Features

* the Focus & reply page in the TUI, on g F as on the web ([#761](https://github.com/planetaryescape/mxr/issues/761)) ([263268b](https://github.com/planetaryescape/mxr/commit/263268b8d7813a71a5a23ddea2abc3aa3ffee92a))


### Bug Fixes

* let main's CI finish, bring the full e2e matrix in line with the app, and time the classifier eval against sync ([#764](https://github.com/planetaryescape/mxr/issues/764)) ([f0150d8](https://github.com/planetaryescape/mxr/commit/f0150d8645928522bc64396e498e82b5f5d07792))

## [0.6.179](https://github.com/planetaryescape/mxr/compare/v0.6.178...v0.6.179) (2026-10-28)


### Features

* Snoozed, wake and the Updates peek in the TUI, with w as wake in both clients ([#760](https://github.com/planetaryescape/mxr/issues/760)) ([6116283](https://github.com/planetaryescape/mxr/commit/6116283f464b2b0ae6a395b0bf1b271fbe09f120))

## [0.6.178](https://github.com/planetaryescape/mxr/compare/v0.6.177...v0.6.178) (2026-10-28)


### Bug Fixes

* rebuild the search index in linear time ([#758](https://github.com/planetaryescape/mxr/issues/758)) ([3e77260](https://github.com/planetaryescape/mxr/commit/3e772608aecd90accf1ffb70b2e7e072f8f7b81f))

## [0.6.177](https://github.com/planetaryescape/mxr/compare/v0.6.176...v0.6.177) (2026-10-28)


### Bug Fixes

* make +word an exact-word match in rules and say what search does ([#756](https://github.com/planetaryescape/mxr/issues/756)) ([a7b8abf](https://github.com/planetaryescape/mxr/commit/a7b8abf58632db45a26f3f757a4904728e937caa))

## [0.6.176](https://github.com/planetaryescape/mxr/compare/v0.6.175...v0.6.176) (2026-10-28)


### Features

* put overlay keys in the key definition and close the web gaps behind the TUI's pass-through panes ([#754](https://github.com/planetaryescape/mxr/issues/754)) ([409a880](https://github.com/planetaryescape/mxr/commit/409a8807d577d4fe901fc851d5bb7ab76c729038))


### Bug Fixes

* keep search answering from SQLite when the lexical index is down ([#753](https://github.com/planetaryescape/mxr/issues/753)) ([8af42e1](https://github.com/planetaryescape/mxr/commit/8af42e156397da4466a186f31050cb51d350dc02))

## [0.6.175](https://github.com/planetaryescape/mxr/compare/v0.6.174...v0.6.175) (2026-10-28)


### Features

* one set of keys in the web app and the TUI, checked from the key definition alone ([#747](https://github.com/planetaryescape/mxr/issues/747)) ([cb436c4](https://github.com/planetaryescape/mxr/commit/cb436c495b94802098f4695ca2363ea57160b81f))

## [0.6.174](https://github.com/planetaryescape/mxr/compare/v0.6.173...v0.6.174) (2026-10-28)


### Bug Fixes

* exit 3 when the daemon is unavailable and 4 on auth errors ([#750](https://github.com/planetaryescape/mxr/issues/750)) ([c7d7977](https://github.com/planetaryescape/mxr/commit/c7d7977652c31b84a092bd8062e5e7d9611f6a06))

## [0.6.173](https://github.com/planetaryescape/mxr/compare/v0.6.172...v0.6.173) (2026-10-28)


### Features

* show what can follow a prefix in the TUI and give the desktop app a menu from the key definition ([#746](https://github.com/planetaryescape/mxr/issues/746)) ([3bb5df2](https://github.com/planetaryescape/mxr/commit/3bb5df25b9b9f9059df9141cf0b26d1b91bb9f1b))


### Bug Fixes

* warn before writing an unsanitized bug report ([#748](https://github.com/planetaryescape/mxr/issues/748)) ([a2b4efd](https://github.com/planetaryescape/mxr/commit/a2b4efde5cedd0129dd773195c65fbd0177de308))

## [0.6.172](https://github.com/planetaryescape/mxr/compare/v0.6.171...v0.6.172) (2026-10-28)


### Features

* mark each email's account in the TUI ([#745](https://github.com/planetaryescape/mxr/issues/745)) ([6b59815](https://github.com/planetaryescape/mxr/commit/6b59815a4ad7eac01ed398f4037bde830999ac4f))
* turn single-key shortcuts off and remap keys in both clients ([#742](https://github.com/planetaryescape/mxr/issues/742)) ([1175f41](https://github.com/planetaryescape/mxr/commit/1175f41609d806e0166c1de4f084d5b79535666f))


### Bug Fixes

* send one unsubscribe request per sender address ([#739](https://github.com/planetaryescape/mxr/issues/739)) ([bd4fb5e](https://github.com/planetaryescape/mxr/commit/bd4fb5e0fd43c1e6831c78108ba110bf6269b390))


### Documentation

* align compose guidance with released voice behavior ([835f06e](https://github.com/planetaryescape/mxr/commit/835f06e051fa8be0ce6f125e5e361aa89ee5158f))

## [0.6.171](https://github.com/planetaryescape/mxr/compare/v0.6.170...v0.6.171) (2026-10-28)


### Features

* build TUI help and the keybindings reference from the key definition ([#740](https://github.com/planetaryescape/mxr/issues/740)) ([e192f92](https://github.com/planetaryescape/mxr/commit/e192f924e5ee1cc04b8bbe5728ba4bc3bcd16a30))


### Documentation

* record the full classifier eval running up against its CI timeout ([b834e15](https://github.com/planetaryescape/mxr/commit/b834e151bc6cf459bc0a10fa34614e58436955a2))

## [0.6.170](https://github.com/planetaryescape/mxr/compare/v0.6.169...v0.6.170) (2026-10-28)


### Features

* digits pick a mode's tabs, Next time teaches keys, and the TUI palette comes from the key definition ([#735](https://github.com/planetaryescape/mxr/issues/735)) ([12a2068](https://github.com/planetaryescape/mxr/commit/12a20682afe5cfc3561132c2252acb5c00946af4))
* unify compose voice and source provenance ([0cfa64b](https://github.com/planetaryescape/mxr/commit/0cfa64bad756e1b749f726a272265f7fecd23fbc))


### Bug Fixes

* show every account's mail in All accounts mailboxes ([#738](https://github.com/planetaryescape/mxr/issues/738)) ([86871a7](https://github.com/planetaryescape/mxr/commit/86871a734fc69adcc2e832333b83f2fa6c2c92f1))


### Documentation

* drop references to the removed impeccable skill and old skill names ([#737](https://github.com/planetaryescape/mxr/issues/737)) ([da0c6c8](https://github.com/planetaryescape/mxr/commit/da0c6c8c3bb6054781cba938c083c726623d5087))

## [0.6.169](https://github.com/planetaryescape/mxr/compare/v0.6.168...v0.6.169) (2026-10-28)


### Features

* one X picker and one Not-now picker in both clients ([#732](https://github.com/planetaryescape/mxr/issues/732)) ([0377c08](https://github.com/planetaryescape/mxr/commit/0377c08489e4c8abc4ae959684aed69b9c33acdd))


### Bug Fixes

* count provider-sent mail as sent even when its From isn't a registered address ([#731](https://github.com/planetaryescape/mxr/issues/731)) ([9e9343b](https://github.com/planetaryescape/mxr/commit/9e9343b562624a8a5960de1351f22b9e15062b02))
* update pulldown-cmark to 0.13.5 for RUSTSEC-2026-0414 ([#734](https://github.com/planetaryescape/mxr/issues/734)) ([2be37e9](https://github.com/planetaryescape/mxr/commit/2be37e94336a8e6bfbe41e426b3e80d96ccde74a))


### Documentation

* point blueprint 17 at the real release workflow and fix drifted compose and issue notes ([#730](https://github.com/planetaryescape/mxr/issues/730)) ([b256d5e](https://github.com/planetaryescape/mxr/commit/b256d5e7957322531bd59bccaef5cced5833d5e3))

## [0.6.168](https://github.com/planetaryescape/mxr/compare/v0.6.167...v0.6.168) (2026-10-28)


### Features

* mark each email's account in the web app ([#728](https://github.com/planetaryescape/mxr/issues/728)) ([3a2e06a](https://github.com/planetaryescape/mxr/commit/3a2e06a16c1001d34776f96eddfefae468589610))


### Documentation

* record inline compose release ([75d1c31](https://github.com/planetaryescape/mxr/commit/75d1c310cf3862f33c5c0c5b69e61bd0bd62f20d))

## [0.6.167](https://github.com/planetaryescape/mxr/compare/v0.6.166...v0.6.167) (2026-10-27)


### Features

* keep replies inline and resume conversation drafts ([b2ea14e](https://github.com/planetaryescape/mxr/commit/b2ea14e8b9a8f4d342cb38af247b409d5109e5bb))

## [0.6.166](https://github.com/planetaryescape/mxr/compare/v0.6.165...v0.6.166) (2026-10-26)


### Bug Fixes

* restore responsive mail views and keyboard navigation ([#722](https://github.com/planetaryescape/mxr/issues/722)) ([2d0d4fe](https://github.com/planetaryescape/mxr/commit/2d0d4fe39adda36cd9cd84841ffc132cf8512c7f))

## [0.6.165](https://github.com/planetaryescape/mxr/compare/v0.6.164...v0.6.165) (2026-10-26)


### Features

* check compose style against the writer’s habits ([3f111fb](https://github.com/planetaryescape/mxr/commit/3f111fb79440068663286de804909e918467ab48))


### Bug Fixes

* preserve forwarded content and retain approved sends through cooldowns ([#717](https://github.com/planetaryescape/mxr/issues/717)) ([0d132b3](https://github.com/planetaryescape/mxr/commit/0d132b378e874016b09df0b7203c21a62f29497a))


### Documentation

* align stored asks guides with shipped behavior ([#719](https://github.com/planetaryescape/mxr/issues/719)) ([bd11232](https://github.com/planetaryescape/mxr/commit/bd11232427941c634da57a0c22d3d866f3634af1))

## [0.6.164](https://github.com/planetaryescape/mxr/compare/v0.6.163...v0.6.164) (2026-10-26)


### Bug Fixes

* make Messages an attention-only queue ([#715](https://github.com/planetaryescape/mxr/issues/715)) ([814b053](https://github.com/planetaryescape/mxr/commit/814b053b120d48034e61c82e14bedf4f6c638d0f))


### Documentation

* record R2c release acceptance ([3e4dd28](https://github.com/planetaryescape/mxr/commit/3e4dd283aee803e25e8a7a1d354c5b91ea7decb1))

## [0.6.163](https://github.com/planetaryescape/mxr/compare/v0.6.162...v0.6.163) (2026-10-26)


### Features

* choose a compose model per draft ([106c05b](https://github.com/planetaryescape/mxr/commit/106c05b361b0f77d2dc483ec54701023d1eb4065))
* unify stored asks and to-do corrections ([fc3e514](https://github.com/planetaryescape/mxr/commit/fc3e514f4281f0c96efc232d67066e2d62b65baf))


### Bug Fixes

* refresh mail before copying the latest code ([ce1d2de](https://github.com/planetaryescape/mxr/commit/ce1d2dee4c65d41e02dda0764e203b37b138dc18))


### Documentation

* clarify Messages audience and verification boundaries ([#713](https://github.com/planetaryescape/mxr/issues/713)) ([69d5b83](https://github.com/planetaryescape/mxr/commit/69d5b833e7a62a90422d74a45108684e7d7a3311))

## [0.6.162](https://github.com/planetaryescape/mxr/compare/v0.6.161...v0.6.162) (2026-10-25)


### Features

* make Messages conversations audience-aware ([#698](https://github.com/planetaryescape/mxr/issues/698)) ([2c18587](https://github.com/planetaryescape/mxr/commit/2c185877802e8bd37c954be3cf14c85b66ddc3d0))


### Bug Fixes

* preserve draft revision after accepting writing help ([0dc65d0](https://github.com/planetaryescape/mxr/commit/0dc65d096c40f17c6b502c2ff835a9dbdc757814))

## [0.6.161](https://github.com/planetaryescape/mxr/compare/v0.6.160...v0.6.161) (2026-10-25)


### Features

* review writing proposals in TUI and Raycast ([64be20a](https://github.com/planetaryescape/mxr/commit/64be20ae34d5a28cb3d9ac0b8f578df3cd2cc757))


### Documentation

* close archived Messages done regression ([#705](https://github.com/planetaryescape/mxr/issues/705)) ([f9d316d](https://github.com/planetaryescape/mxr/commit/f9d316de44c4081d2a5f314c54763525d5884d83))

## [0.6.160](https://github.com/planetaryescape/mxr/compare/v0.6.159...v0.6.160) (2026-10-23)


### Bug Fixes

* allow done on archived Messages topics ([#703](https://github.com/planetaryescape/mxr/issues/703)) ([5213b49](https://github.com/planetaryescape/mxr/commit/5213b493b7e7be40584b7f0bdf8d6a76158cc458))


### Documentation

* align discovery and synthetic recordings with mxr runtime ([a9b1346](https://github.com/planetaryescape/mxr/commit/a9b1346f763e7f43a1ece10f50c63b74140f0f12))
* connect the five mail jobs to the shared runtime ([16b1ada](https://github.com/planetaryescape/mxr/commit/16b1adaed540bd0fdd9e2d447ecd6f6711abcf60))
* explain local workflows and daemon authority ([cd08cfe](https://github.com/planetaryescape/mxr/commit/cd08cfe5eb72b0959bc623ad15439bbf09cbe2c3))
* reconcile runtime knowledge and example semantics ([#701](https://github.com/planetaryescape/mxr/issues/701)) ([544da53](https://github.com/planetaryescape/mxr/commit/544da5355fbaa11a7cf64e0fecbd844ca119623a))
* record completed runtime website rollout ([b3c2f3b](https://github.com/planetaryescape/mxr/commit/b3c2f3b00aca85c2c7375030e86fb7f2380ba4fc))

## [0.6.159](https://github.com/planetaryescape/mxr/compare/v0.6.158...v0.6.159) (2026-10-23)


### Features

* inspect Messages identities and message audiences ([748044a](https://github.com/planetaryescape/mxr/commit/748044ab455371aa73e36f344e4410e63130f78d))
* share one stored classifier assessment across mxr ([#687](https://github.com/planetaryescape/mxr/issues/687)) ([fe8ea0f](https://github.com/planetaryescape/mxr/commit/fe8ea0f1ba4253df34d586c82eb2a58e1a240695))


### Documentation

* guide existing agents through shared draft review ([4874fbd](https://github.com/planetaryescape/mxr/commit/4874fbd31d785e9e37d64080b2ba1db696e689dd))

## [0.6.158](https://github.com/planetaryescape/mxr/compare/v0.6.157...v0.6.158) (2026-10-23)


### Bug Fixes

* require desktop artifacts before release publication ([7636657](https://github.com/planetaryescape/mxr/commit/7636657ac48dae1846eb1e3d1c0a7c20ce8ca0c0))

## [0.6.157](https://github.com/planetaryescape/mxr/compare/v0.6.156...v0.6.157) (2026-10-23)


### Features

* build writing help on the current draft ([5fd6382](https://github.com/planetaryescape/mxr/commit/5fd638271cf6598797f80c7d9ab83d7a935a6d1a))


### Documentation

* present mxr as a shared local mail runtime ([cc00d40](https://github.com/planetaryescape/mxr/commit/cc00d4081bb5bd7875c39da4641ff95888817d80))

## [0.6.156](https://github.com/planetaryescape/mxr/compare/v0.6.155...v0.6.156) (2026-10-23)


### Features

* move the rest of the verb table's single keys ([#683](https://github.com/planetaryescape/mxr/issues/683)) ([930f1c4](https://github.com/planetaryescape/mxr/commit/930f1c4a70e97939004b552352c9dba994fa7c18))

## [0.6.155](https://github.com/planetaryescape/mxr/compare/v0.6.154...v0.6.155) (2026-10-23)


### Features

* open tools with g and a capital, and retire the duplicate keys ([#681](https://github.com/planetaryescape/mxr/issues/681)) ([00b7135](https://github.com/planetaryescape/mxr/commit/00b7135fb99fc6ef5ec3442d8a3c2e951d0b2e4c))

## [0.6.154](https://github.com/planetaryescape/mxr/compare/v0.6.153...v0.6.154) (2026-10-23)


### Features

* move the first keys to the chapter 35 map, announcing each old key ([#676](https://github.com/planetaryescape/mxr/issues/676)) ([348789d](https://github.com/planetaryescape/mxr/commit/348789d3b08a718c08f5bda6eeec2c0a54e58496))


### Bug Fixes

* pin the full classifier eval's clock so it grades the same at any hour ([#680](https://github.com/planetaryescape/mxr/issues/680)) ([7a8d624](https://github.com/planetaryescape/mxr/commit/7a8d624bad1e42d093170d4bcad9400ca4422832))
* reclassify stored mail when an account's addresses change ([#665](https://github.com/planetaryescape/mxr/issues/665)) ([015f9e8](https://github.com/planetaryescape/mxr/commit/015f9e8ae2065c9c8c5e20e8e18a5b78aaeadd6d))


### Documentation

* record that the full classifier eval depends on the time of day ([6085bce](https://github.com/planetaryescape/mxr/commit/6085bce141af6ee4090eb38857e36a90271a0a2e))

## [0.6.153](https://github.com/planetaryescape/mxr/compare/v0.6.152...v0.6.153) (2026-10-23)


### Bug Fixes

* cap assistant replies at 16k tokens and never show a provider's error body ([#677](https://github.com/planetaryescape/mxr/issues/677)) ([57ec457](https://github.com/planetaryescape/mxr/commit/57ec4576162a50845403da02fb36778d1941e32f))

## [0.6.152](https://github.com/planetaryescape/mxr/compare/v0.6.151...v0.6.152) (2026-10-23)


### Features

* grade classify eval by kind of ask and by field ([#664](https://github.com/planetaryescape/mxr/issues/664)) ([81875ca](https://github.com/planetaryescape/mxr/commit/81875ca9ace4a2a580be88b2d0ba51eed222cc76))


### Bug Fixes

* keep the phone list usable under a docked reply and fix three stale e2e checks ([#673](https://github.com/planetaryescape/mxr/issues/673)) ([4b08c36](https://github.com/planetaryescape/mxr/commit/4b08c36069fb1916cea5b49dd5e8c20451ed2279))
* write due mismatches from the label so the quick snapshot passes at any hour ([#675](https://github.com/planetaryescape/mxr/issues/675)) ([1dda1ce](https://github.com/planetaryescape/mxr/commit/1dda1ce5422517fb0bc1cd98be3300185a1a8192))


### Documentation

* cover everything shipped from 0.6.120 to 0.6.150 on the docs site ([#669](https://github.com/planetaryescape/mxr/issues/669)) ([18763dd](https://github.com/planetaryescape/mxr/commit/18763ddbe21ea6dd57b1defa36314b7ce90a9005))
* track the WebKit spec failures and close the Mac rust-cache issue ([#671](https://github.com/planetaryescape/mxr/issues/671)) ([ca5f2e4](https://github.com/planetaryescape/mxr/commit/ca5f2e47eb132f6cd51f5921e28cffdf9ebbcc94))

## [0.6.151](https://github.com/planetaryescape/mxr/compare/v0.6.150...v0.6.151) (2026-10-22)


### Bug Fixes

* make every mode guide show keys that work there, from the key definition ([#661](https://github.com/planetaryescape/mxr/issues/661)) ([eda0661](https://github.com/planetaryescape/mxr/commit/eda066153670aa557245736220f4479890c3d151))


### Documentation

* say release builds never enable the langfuse feature ([#668](https://github.com/planetaryescape/mxr/issues/668)) ([aa26dd2](https://github.com/planetaryescape/mxr/commit/aa26dd25b7749d44b3f0a5e3b0dc84243b13c634))

## [0.6.150](https://github.com/planetaryescape/mxr/compare/v0.6.149...v0.6.150) (2026-10-22)


### Features

* filter Messages to unread, your turn, waiting on them or past your pace, in every client ([#658](https://github.com/planetaryescape/mxr/issues/658)) ([4379ce7](https://github.com/planetaryescape/mxr/commit/4379ce703a9385260703f91366c93d630957210a))
* put the composer's and arrivals' keys in the key definition, and fail on stray key listeners ([#660](https://github.com/planetaryescape/mxr/issues/660)) ([faf4341](https://github.com/planetaryescape/mxr/commit/faf434132929aba364e8e453ecc45f0c6cc6e2f4))


### Bug Fixes

* run only the cross-engine WebKit subset on the Mac runner ([#662](https://github.com/planetaryescape/mxr/issues/662)) ([53b6b68](https://github.com/planetaryescape/mxr/commit/53b6b68f3af3e8cecf0739b3d59c842eb4086394))

## [0.6.149](https://github.com/planetaryescape/mxr/compare/v0.6.148...v0.6.149) (2026-10-22)


### Features

* give the Raycast extension the same keys as the TUI and the web app ([#655](https://github.com/planetaryescape/mxr/issues/655)) ([43bc64c](https://github.com/planetaryescape/mxr/commit/43bc64c5ce3987ff441f0c6eccdcbdc388b48bb2))
* grade Draft for me on a labelled corpus with mxr draft eval --set quick ([#654](https://github.com/planetaryescape/mxr/issues/654)) ([0bcdbba](https://github.com/planetaryescape/mxr/commit/0bcdbba706557ccd3f8f4d6f9c3fd7381c4ac64f))


### Bug Fixes

* make TUI keys safe: Esc keeps the draft, q steps back, deletes ask, paste and Ctrl-z behave ([#656](https://github.com/planetaryescape/mxr/issues/656)) ([0a60db2](https://github.com/planetaryescape/mxr/commit/0a60db265159b586e3a78a09466cbba420046b83))
* make web keys safe while typing, on any layout, and after menus close ([#652](https://github.com/planetaryescape/mxr/issues/652)) ([9636ffc](https://github.com/planetaryescape/mxr/commit/9636ffcf925057b4427153f5c3ce70a08eac4854))


### Documentation

* put tracing a developer build on the docs site ([#659](https://github.com/planetaryescape/mxr/issues/659)) ([d4810f3](https://github.com/planetaryescape/mxr/commit/d4810f3ee1f1108aca34678524b3cea4df736787))

## [0.6.148](https://github.com/planetaryescape/mxr/compare/v0.6.147...v0.6.148) (2026-10-22)


### Features

* remap a mode's keys in keys.toml, check the file, and list each mode's keys in help ([#646](https://github.com/planetaryescape/mxr/issues/646)) ([15bb54c](https://github.com/planetaryescape/mxr/commit/15bb54ced20e7033d8f4540d3cdb265c1624525d))


### Bug Fixes

* drop dead web place keys, read nav key labels from the definition, and publish every mode's keys ([#648](https://github.com/planetaryescape/mxr/issues/648)) ([2f56e5e](https://github.com/planetaryescape/mxr/commit/2f56e5e8de5fe754ca704dd22f114cd1b46d51ba))
* move the web app to dompurify 3.4.17 for three sanitizer advisories ([#649](https://github.com/planetaryescape/mxr/issues/649)) ([208ff5a](https://github.com/planetaryescape/mxr/commit/208ff5ab3b9bf312e7f9d7ba955429b9403d99ee))
* suggest each contact once in compose To ([#650](https://github.com/planetaryescape/mxr/issues/650)) ([53ca64a](https://github.com/planetaryescape/mxr/commit/53ca64a61b3157f201af9878df1c4ee5e2ec6004))

## [0.6.147](https://github.com/planetaryescape/mxr/compare/v0.6.146...v0.6.147) (2026-10-22)


### Bug Fixes

* suggest contacts in To without crashing compose ([#641](https://github.com/planetaryescape/mxr/issues/641)) ([dc271b1](https://github.com/planetaryescape/mxr/commit/dc271b1b77743d4736c7c4cb5feaf168f1635d5e))

## [0.6.146](https://github.com/planetaryescape/mxr/compare/v0.6.145...v0.6.146) (2026-10-22)


### Features

* grade all 50,000 demo emails with mxr classify eval --set full ([#638](https://github.com/planetaryescape/mxr/issues/638)) ([a258c9d](https://github.com/planetaryescape/mxr/commit/a258c9de7731c22ce70355f166e45a4b0aaa38b7))


### Bug Fixes

* keep CI's WebKit job on the Mac runner off the developer's toolchain ([#644](https://github.com/planetaryescape/mxr/issues/644)) ([36e3b47](https://github.com/planetaryescape/mxr/commit/36e3b47db68b1a84c8dd46dc83f78108183c4d15))
* show each TUI mode's own verbs, and dispatch its keys from the key definition ([#639](https://github.com/planetaryescape/mxr/issues/639)) ([4a9e838](https://github.com/planetaryescape/mxr/commit/4a9e838a1bf85f8ced6e92dc3cc212324f457dac))


### Refactoring

* read web shortcuts from the key definition ([#633](https://github.com/planetaryescape/mxr/issues/633)) ([8cb0388](https://github.com/planetaryescape/mxr/commit/8cb038890090e2dc74bc63341f68167e31e1a363))


### Documentation

* design the What-it-asks judge as blueprint chapter 37 ([b1a208a](https://github.com/planetaryescape/mxr/commit/b1a208a20834b9e9f4f7ce552bc80f6bb2cf1db5))

## [0.6.145](https://github.com/planetaryescape/mxr/compare/v0.6.144...v0.6.145) (2026-10-22)


### Features

* list every keyboard shortcut from one definition with mxr keys ([#632](https://github.com/planetaryescape/mxr/issues/632)) ([b2bdc40](https://github.com/planetaryescape/mxr/commit/b2bdc40d9eafed78f2fa092c755c2fede080147e))


### Bug Fixes

* keep sync, cached AI results and the TUI current after an address change ([#631](https://github.com/planetaryescape/mxr/issues/631)) ([21c61d5](https://github.com/planetaryescape/mxr/commit/21c61d5f0266f6c22f32152fd36217cb766b7d65))


### Documentation

* remove a stray merge marker from the decision log ([#640](https://github.com/planetaryescape/mxr/issues/640)) ([67822bc](https://github.com/planetaryescape/mxr/commit/67822bc46aa02d4989f70398fdf8d85e8a4b69df))

## [0.6.144](https://github.com/planetaryescape/mxr/compare/v0.6.143...v0.6.144) (2026-10-22)


### Bug Fixes

* bump vitest to 4.1.12 to clear the two mocker advisories ([#634](https://github.com/planetaryescape/mxr/issues/634)) ([6e893ad](https://github.com/planetaryescape/mxr/commit/6e893adf0120a4fbd8d23c74d0a131cbed086cd3))

## [0.6.143](https://github.com/planetaryescape/mxr/compare/v0.6.142...v0.6.143) (2026-10-22)


### Features

* give every account a colour and a tag ([#628](https://github.com/planetaryescape/mxr/issues/628)) ([54908a4](https://github.com/planetaryescape/mxr/commit/54908a44cab1488aa152ffdc2d8e6813ec019377))
* keep history to 90 days by pruning it daily ([#626](https://github.com/planetaryescape/mxr/issues/626)) ([f14f0b0](https://github.com/planetaryescape/mxr/commit/f14f0b0b3f5362f0132139c71231e1889a57bf2d))


### Bug Fixes

* keep the Mac release build off the developer's Rust toolchain and sccache ([#629](https://github.com/planetaryescape/mxr/issues/629)) ([a4bf492](https://github.com/planetaryescape/mxr/commit/a4bf49248ccca2612bde57eac4204a9580f0e7b2))
* make main pass fmt, clippy, rustdoc and the OpenAPI snapshot again ([#630](https://github.com/planetaryescape/mxr/issues/630)) ([0e1d81a](https://github.com/planetaryescape/mxr/commit/0e1d81ad1881f97f07c548c155cfffcab7ae7db3))


### Documentation

* design keyboard shortcuts as one grammar across every client ([d964546](https://github.com/planetaryescape/mxr/commit/d9645466096fa00dccb75bc507a5bbf6009a1b09))
* design the composer and its compose screens ([e9079f4](https://github.com/planetaryescape/mxr/commit/e9079f46452a0b9b2e8168dd86269657c7dc9503))

## [0.6.142](https://github.com/planetaryescape/mxr/compare/v0.6.141...v0.6.142) (2026-10-22)


### Features

* thread previews as CSV and as an MCP tool for the assistant ([#622](https://github.com/planetaryescape/mxr/issues/622)) ([7184494](https://github.com/planetaryescape/mxr/commit/718449494330e1fad3f50d4fe44ae1f9f282f1c9))


### Bug Fixes

* one owned-address set for every daemon feature ([#624](https://github.com/planetaryescape/mxr/issues/624)) ([8cd1558](https://github.com/planetaryescape/mxr/commit/8cd1558c88ee304e8286ed5a1831a5eef194b3c9))


### Documentation

* remove merge leftovers from the decision log ([#623](https://github.com/planetaryescape/mxr/issues/623)) ([e06c409](https://github.com/planetaryescape/mxr/commit/e06c4094599d30809fdf57dc9e0aae164aa1e940))

## [0.6.141](https://github.com/planetaryescape/mxr/compare/v0.6.140...v0.6.141) (2026-10-22)


### Features

* act only on commitment dates read from the email's words or typed by the user ([#616](https://github.com/planetaryescape/mxr/issues/616)) ([e4898aa](https://github.com/planetaryescape/mxr/commit/e4898aa2cf033738f7dac441394cf868c6647b58))


### Bug Fixes

* run the web unit tests in one locale so CI formats dates like a British laptop ([#618](https://github.com/planetaryescape/mxr/issues/618)) ([cad87b3](https://github.com/planetaryescape/mxr/commit/cad87b396efad74831ddaa338545f93754ef0f8d))
* satisfy clippy in the TUI arrival-time snapshot tests ([#621](https://github.com/planetaryescape/mxr/issues/621)) ([11d52fe](https://github.com/planetaryescape/mxr/commit/11d52fecc2e496b18f1d0b5f22158b2a8ba61ee5))

## [0.6.140](https://github.com/planetaryescape/mxr/compare/v0.6.139...v0.6.140) (2026-10-22)


### Features

* show a card when hovering an email or person link in the assistant ([#613](https://github.com/planetaryescape/mxr/issues/613)) ([ac24399](https://github.com/planetaryescape/mxr/commit/ac243993327cbf462eca1eadd8d710893dc96059))


### Bug Fixes

* show when an email arrived in local time across the TUI ([#619](https://github.com/planetaryescape/mxr/issues/619)) ([4457c06](https://github.com/planetaryescape/mxr/commit/4457c06cdb61c2a623fa005e4ef287200574b85b))

## [0.6.139](https://github.com/planetaryescape/mxr/compare/v0.6.138...v0.6.139) (2026-10-22)


### Features

* read, search and filter by account in the Raycast extension ([#594](https://github.com/planetaryescape/mxr/issues/594)) ([6851b2a](https://github.com/planetaryescape/mxr/commit/6851b2ab97a8a36e04b9b70f8e761cd480bb4ea7))

## [0.6.138](https://github.com/planetaryescape/mxr/compare/v0.6.137...v0.6.138) (2026-10-22)


### Features

* find the one-time code in sign-in mail and make it one key to copy ([#608](https://github.com/planetaryescape/mxr/issues/608)) ([d98b8ad](https://github.com/planetaryescape/mxr/commit/d98b8adfcc5415f7ad217dd1895c724500d10846))

## [0.6.137](https://github.com/planetaryescape/mxr/compare/v0.6.136...v0.6.137) (2026-10-22)


### Features

* draw assistant email rows as compact cards ([#610](https://github.com/planetaryescape/mxr/issues/610)) ([b69a4e6](https://github.com/planetaryescape/mxr/commit/b69a4e6448ebedf0dcd05d4fa282f67284c13332))

## [0.6.136](https://github.com/planetaryescape/mxr/compare/v0.6.135...v0.6.136) (2026-10-22)


### Bug Fixes

* never send an unsubscribe twice after logs are purged ([#606](https://github.com/planetaryescape/mxr/issues/606)) ([2053235](https://github.com/planetaryescape/mxr/commit/20532357545cecc15dac6ad6971fed4f75edb2c7))

## [0.6.135](https://github.com/planetaryescape/mxr/compare/v0.6.134...v0.6.135) (2026-10-22)


### Bug Fixes

* stop renewing Gmail rate limits by waiting Gmail's full retry time ([#604](https://github.com/planetaryescape/mxr/issues/604)) ([4875051](https://github.com/planetaryescape/mxr/commit/48750519a055008a5c6b2965bd027336543e25f5))

## [0.6.134](https://github.com/planetaryescape/mxr/compare/v0.6.133...v0.6.134) (2026-10-22)


### Features

* show what the assistant is doing while mxr assistant send waits ([#601](https://github.com/planetaryescape/mxr/issues/601)) ([fd55bf3](https://github.com/planetaryescape/mxr/commit/fd55bf3aa42ce9c5907fa830968ef8b3a2d1eae7))


### Bug Fixes

* pad 24-hour times and give the arrival its own line on phones ([#602](https://github.com/planetaryescape/mxr/issues/602)) ([91838af](https://github.com/planetaryescape/mxr/commit/91838afb13900852ed21cb475bcdbe5434ba6fab))

## [0.6.133](https://github.com/planetaryescape/mxr/compare/v0.6.132...v0.6.133) (2026-10-22)


### Bug Fixes

* never serve a mode view that may predate synced mail, and stop rebuilding unchanged views in the background ([#595](https://github.com/planetaryescape/mxr/issues/595)) ([06e3361](https://github.com/planetaryescape/mxr/commit/06e33616c272e92bda5699133378743cf4acdec8))

## [0.6.132](https://github.com/planetaryescape/mxr/compare/v0.6.131...v0.6.132) (2026-10-22)


### Bug Fixes

* always show when an open email arrived in the web app ([#598](https://github.com/planetaryescape/mxr/issues/598)) ([32f508a](https://github.com/planetaryescape/mxr/commit/32f508ab5f178f2be35c59114e8b8c57655de42a))


### Documentation

* give each account one colour and tag, owned by the daemon ([#596](https://github.com/planetaryescape/mxr/issues/596)) ([8c05bcb](https://github.com/planetaryescape/mxr/commit/8c05bcb066c478c7abd678aec4cd53525b57c6ad))
* hold a build slot only while something builds ([3bc28e3](https://github.com/planetaryescape/mxr/commit/3bc28e3443086a550f2d623dfc421aff53ffae21))
* show when an open email arrived, always and in plain sight ([#597](https://github.com/planetaryescape/mxr/issues/597)) ([e7ecc14](https://github.com/planetaryescape/mxr/commit/e7ecc1410335f2bca460bc341d2fedefe78107e6))

## [0.6.131](https://github.com/planetaryescape/mxr/compare/v0.6.130...v0.6.131) (2026-10-22)


### Features

* doctor shows recent sync durations and repeated errors ([#592](https://github.com/planetaryescape/mxr/issues/592)) ([484169a](https://github.com/planetaryescape/mxr/commit/484169a3a1787adad55daf29fa02fcf3597c640b))

## [0.6.130](https://github.com/planetaryescape/mxr/compare/v0.6.129...v0.6.130) (2026-10-22)


### Bug Fixes

* make Undo Send exact and refuse sends an account cannot make ([#588](https://github.com/planetaryescape/mxr/issues/588)) ([f3a5323](https://github.com/planetaryescape/mxr/commit/f3a5323475324df5136cd8c5c2046fa29d0c180e))

## [0.6.129](https://github.com/planetaryescape/mxr/compare/v0.6.128...v0.6.129) (2026-10-22)


### Bug Fixes

* show why Gmail rate-limits an account and wait what it asks ([#589](https://github.com/planetaryescape/mxr/issues/589)) ([d0be767](https://github.com/planetaryescape/mxr/commit/d0be76773ce695bf55f534bb3ddc711af7109983))

## [0.6.128](https://github.com/planetaryescape/mxr/compare/v0.6.127...v0.6.128) (2026-10-21)


### Features

* work through the five modes from Raycast ([#582](https://github.com/planetaryescape/mxr/issues/582)) ([7b3d50c](https://github.com/planetaryescape/mxr/commit/7b3d50c71d968d9c16c9eaa215f9ee7d8ed72922))


### Bug Fixes

* one icon per mode across the web app ([#586](https://github.com/planetaryescape/mxr/issues/586)) ([5b04d77](https://github.com/planetaryescape/mxr/commit/5b04d77a258e7007c6a410f2ae5e8fa099161ee8))
* play the finishing scene once a day on Now and Reading ([#585](https://github.com/planetaryescape/mxr/issues/585)) ([6a76a49](https://github.com/planetaryescape/mxr/commit/6a76a49b8a3d1b48402e4fdf45c360c52186b40c))

## [0.6.127](https://github.com/planetaryescape/mxr/compare/v0.6.126...v0.6.127) (2026-10-21)


### Features

* open everything an assistant step found ([#584](https://github.com/planetaryescape/mxr/issues/584)) ([d08625d](https://github.com/planetaryescape/mxr/commit/d08625d2553e0207b54d30313c7aac40d1e4f185))


### Bug Fixes

* keep Compose whole in the phone top bar ([#578](https://github.com/planetaryescape/mxr/issues/578)) ([bcb007a](https://github.com/planetaryescape/mxr/commit/bcb007a3395097e17ac0c53ef94a5c999733977f))
* partial trash toast e2e, TUI O override, settings fade, journey 2 evidence ([#577](https://github.com/planetaryescape/mxr/issues/577)) ([8da770b](https://github.com/planetaryescape/mxr/commit/8da770b74b91b6c8c1a15ce262a8fc9b8c637e94))
* reply to your own sent message goes to its recipients ([#580](https://github.com/planetaryescape/mxr/issues/580)) ([2b580fc](https://github.com/planetaryescape/mxr/commit/2b580fcf8a2c671948271c3bfb6558d4d22b51a9))
* stop idle syncs rebuilding every cached mode view ([#581](https://github.com/planetaryescape/mxr/issues/581)) ([9b80ad8](https://github.com/planetaryescape/mxr/commit/9b80ad883db7127ff1d0c7f1d8372ea4b6b4a65b))


### Documentation

* warn agents about the known Vitest audit failure ([#583](https://github.com/planetaryescape/mxr/issues/583)) ([bde2da5](https://github.com/planetaryescape/mxr/commit/bde2da5e938b94b7b89ab55b525e52a734123173))

## [0.6.126](https://github.com/planetaryescape/mxr/compare/v0.6.125...v0.6.126) (2026-10-21)


### Features

* list every message an assistant change touches in the TUI ([#576](https://github.com/planetaryescape/mxr/issues/576)) ([06214a7](https://github.com/planetaryescape/mxr/commit/06214a73ca7e0803354161585dd9d87fdf74ee28))
* pick the assistant's model, local or cloud ([#572](https://github.com/planetaryescape/mxr/issues/572)) ([593e67b](https://github.com/planetaryescape/mxr/commit/593e67b5f8c3de42d88b87bb9ca9553207492694))


### Bug Fixes

* link names a reply quotes from a step's rows ([#570](https://github.com/planetaryescape/mxr/issues/570)) ([3b679c5](https://github.com/planetaryescape/mxr/commit/3b679c5d182ff704c108acc1ff746040e5aaead0))


### Documentation

* allow six build and test workers per queued job ([381e555](https://github.com/planetaryescape/mxr/commit/381e55544ea984a620f0990813610828d0ba9b57))
* three queued jobs of five workers each ([0d939ce](https://github.com/planetaryescape/mxr/commit/0d939cebaa2fa0442165565af7b69fb5f7163a06))

## [0.6.125](https://github.com/planetaryescape/mxr/compare/v0.6.124...v0.6.125) (2026-10-21)


### Features

* retire five settings that never did anything ([#573](https://github.com/planetaryescape/mxr/issues/573)) ([ed00a6b](https://github.com/planetaryescape/mxr/commit/ed00a6b47a0591732206922a9533b822c6ea38da))

## [0.6.124](https://github.com/planetaryescape/mxr/compare/v0.6.123...v0.6.124) (2026-10-21)


### Features

* answer Now and mode reads from ready-made views ([#567](https://github.com/planetaryescape/mxr/issues/567)) ([ecffbc9](https://github.com/planetaryescape/mxr/commit/ecffbc966cead4a1081c52553e4cfb31861d11b5))

## [0.6.123](https://github.com/planetaryescape/mxr/compare/v0.6.122...v0.6.123) (2026-10-21)


### Features

* search honours its configured defaults ([#568](https://github.com/planetaryescape/mxr/issues/568)) ([db7d768](https://github.com/planetaryescape/mxr/commit/db7d76804899d40bbc40de4bdc11d2c43130ac76))


### Bug Fixes

* archive at once in the web app ([#565](https://github.com/planetaryescape/mxr/issues/565)) ([477206d](https://github.com/planetaryescape/mxr/commit/477206dc043505008ac54b97cc662c8ea5615fef))
* speed up mode reads by classifying each message once ([#564](https://github.com/planetaryescape/mxr/issues/564)) ([0e5db2e](https://github.com/planetaryescape/mxr/commit/0e5db2e1584fed17aff4444a00a1dbe9e023933e))


### Documentation

* fit server labels into the classifier as a surface, not a feature ([3fdd3a8](https://github.com/planetaryescape/mxr/commit/3fdd3a888c281e5693693c0ddb47098ae81b2d26))
* turn blueprint 32's build order into the release ladder ([3b61579](https://github.com/planetaryescape/mxr/commit/3b61579a3f34203bdacb9954612b05401fa2e25c))
* write mxr's sorting back to the mail server as two-way labels ([f18741d](https://github.com/planetaryescape/mxr/commit/f18741dbf17b776848a15a840e5d30950489bd41))

## [0.6.122](https://github.com/planetaryescape/mxr/compare/v0.6.121...v0.6.122) (2026-10-21)


### Features

* grade the classifier against labelled scenarios with mxr classify eval ([#561](https://github.com/planetaryescape/mxr/issues/561)) ([6e22fa8](https://github.com/planetaryescape/mxr/commit/6e22fa84a171356ba7c4050f6ef0da6123e92ebe))

## [0.6.121](https://github.com/planetaryescape/mxr/compare/v0.6.120...v0.6.121) (2026-10-21)


### Bug Fixes

* do clock-change-proof calendar-day arithmetic in to-do lead times ([#558](https://github.com/planetaryescape/mxr/issues/558)) ([5854383](https://github.com/planetaryescape/mxr/commit/585438304cce12a08abe585d21cb998f4ba64d4f))
* resolve decision and delivery dates from the email's own words ([#560](https://github.com/planetaryescape/mxr/issues/560)) ([3467e50](https://github.com/planetaryescape/mxr/commit/3467e506903b2dcb09867e1d524cd9e2a510e38b))

## [0.6.120](https://github.com/planetaryescape/mxr/compare/v0.6.119...v0.6.120) (2026-10-21)


### Bug Fixes

* date commitments from the email's own due words, not a model date ([#555](https://github.com/planetaryescape/mxr/issues/555)) ([5ac60d8](https://github.com/planetaryescape/mxr/commit/5ac60d86929785dd723a4a3927516238275f2b55))
* index thread lookups so mode reads stop starving the reader pool ([#556](https://github.com/planetaryescape/mxr/issues/556)) ([af9c952](https://github.com/planetaryescape/mxr/commit/af9c952d7f3b9399453263ebf272aaf9315fae2f))

## [0.6.119](https://github.com/planetaryescape/mxr/compare/v0.6.118...v0.6.119) (2026-10-21)


### Features

* select by pattern, count visual rows, preview bulk confirms ([#554](https://github.com/planetaryescape/mxr/issues/554)) ([70d6852](https://github.com/planetaryescape/mxr/commit/70d685227cb43971812d39b9df466d2a92628183))


### Bug Fixes

* report the real cause when a Gmail background token refresh fails ([#550](https://github.com/planetaryescape/mxr/issues/550)) ([c913fb0](https://github.com/planetaryescape/mxr/commit/c913fb0409c825528c14dec48a9f0a917ba6b5f0))


### Documentation

* add the 2026-10-20 blueprint implementation audit ([6bbb5df](https://github.com/planetaryescape/mxr/commit/6bbb5dfda24817af0dafb8f5e24e48aed7c765f7))
* design one classifier and attention system for the modes ([ec94e1f](https://github.com/planetaryescape/mxr/commit/ec94e1f193eab520052c6f182b879a104be3ffba))

## [0.6.118](https://github.com/planetaryescape/mxr/compare/v0.6.117...v0.6.118) (2026-10-21)


### Features

* pick export format and destination from the TUI ([#548](https://github.com/planetaryescape/mxr/issues/548)) ([18d3fcd](https://github.com/planetaryescape/mxr/commit/18d3fcd305bf775e27af74116a47618ba839db4e))


### Documentation

* record verified fullscreen release ([fd81896](https://github.com/planetaryescape/mxr/commit/fd818964c2d5fad46f8ac04f01d1c904a33d6ddc))

## [0.6.117](https://github.com/planetaryescape/mxr/compare/v0.6.116...v0.6.117) (2026-10-21)


### Bug Fixes

* unblock release proof and web dependency audit ([8bfe7f7](https://github.com/planetaryescape/mxr/commit/8bfe7f7289484fb521be429dfe083fd5712b3df1))

## [0.6.116](https://github.com/planetaryescape/mxr/compare/v0.6.115...v0.6.116) (2026-10-20)


### Bug Fixes

* use the full mail area for fullscreen reader ([36c88ac](https://github.com/planetaryescape/mxr/commit/36c88ac87c130862d463a7a10ee5f7a9267d97d2))

## [0.6.115](https://github.com/planetaryescape/mxr/compare/v0.6.114...v0.6.115) (2026-10-20)


### Performance

* run native webkit on m3 max runner and drop from pr gates ([da76665](https://github.com/planetaryescape/mxr/commit/da76665746610e3fd143b6122977c5adbdb485a9))

## [0.6.114](https://github.com/planetaryescape/mxr/compare/v0.6.113...v0.6.114) (2026-10-20)


### Bug Fixes

* bound local logs and enforce OpenAPI artifact freshness ([#537](https://github.com/planetaryescape/mxr/issues/537)) ([83b54c0](https://github.com/planetaryescape/mxr/commit/83b54c0a312c32f85bee9b9f301f617b28d14535))

## [0.6.113](https://github.com/planetaryescape/mxr/compare/v0.6.112...v0.6.113) (2026-10-20)


### Performance

* tier and shard web e2e suites with prebuilt daemon ([bc565c0](https://github.com/planetaryescape/mxr/commit/bc565c02e3e1c53a426f604489c6b3bd6f78493d))

## [0.6.112](https://github.com/planetaryescape/mxr/compare/v0.6.111...v0.6.112) (2026-10-20)


### Bug Fixes

* cover the visible CLI help tree and reference examples ([#532](https://github.com/planetaryescape/mxr/issues/532)) ([164271a](https://github.com/planetaryescape/mxr/commit/164271ac0ae6a6ba5434821d066f6503c78a77f2))
* isolate account label recovery across backfill pages ([#535](https://github.com/planetaryescape/mxr/issues/535)) ([b80f492](https://github.com/planetaryescape/mxr/commit/b80f4929d924c713cdca2156d17e790ee7fd7cef))
* separate message purpose from reply obligations ([4b6c6bc](https://github.com/planetaryescape/mxr/commit/4b6c6bca1d9d2d489de14ceba25df5e2ec3ee0e7))


### Documentation

* support symlinked CLI reference generation ([#534](https://github.com/planetaryescape/mxr/issues/534)) ([92c69ec](https://github.com/planetaryescape/mxr/commit/92c69ec821d7d2b88e53c228fb7667de8dd3bf9f))

## [0.6.111](https://github.com/planetaryescape/mxr/compare/v0.6.110...v0.6.111) (2026-10-20)


### Bug Fixes

* align rule previews and deliver stored mail to hooks ([#530](https://github.com/planetaryescape/mxr/issues/530)) ([3c47791](https://github.com/planetaryescape/mxr/commit/3c47791a384025802fb903f1a60a5234c293eb77))

## [0.6.110](https://github.com/planetaryescape/mxr/compare/v0.6.109...v0.6.110) (2026-10-20)


### Bug Fixes

* honor configured TUI mail shortcuts ([#528](https://github.com/planetaryescape/mxr/issues/528)) ([bcee881](https://github.com/planetaryescape/mxr/commit/bcee881b3e27c8c6bf33c25483718459c2d5ed23))

## [0.6.109](https://github.com/planetaryescape/mxr/compare/v0.6.108...v0.6.109) (2026-10-20)


### Bug Fixes

* honor activity recording privacy controls ([#526](https://github.com/planetaryescape/mxr/issues/526)) ([b9840d3](https://github.com/planetaryescape/mxr/commit/b9840d384d59b21b88e5740e5d07f063c4f20204))


### Documentation

* align mode guides with shipped workflows ([#525](https://github.com/planetaryescape/mxr/issues/525)) ([f83c039](https://github.com/planetaryescape/mxr/commit/f83c03992eed4ad5848781aaf4cd8515e6912169))
* document local build resource limits in AGENTS.md ([#524](https://github.com/planetaryescape/mxr/issues/524)) ([404c223](https://github.com/planetaryescape/mxr/commit/404c223d4cb66714fa9750cdd8eefc5c935ab1ee))

## [0.6.108](https://github.com/planetaryescape/mxr/compare/v0.6.107...v0.6.108) (2026-10-19)


### Bug Fixes

* keep Gmail background refresh noninteractive ([#522](https://github.com/planetaryescape/mxr/issues/522)) ([214e69f](https://github.com/planetaryescape/mxr/commit/214e69f9ba43f0b8667b6b2d2c94efdfa2e7825a))

## [0.6.107](https://github.com/planetaryescape/mxr/compare/v0.6.106...v0.6.107) (2026-10-19)


### Features

* link Mixer tasks with Microsoft To Do ([#520](https://github.com/planetaryescape/mxr/issues/520)) ([9ca8f08](https://github.com/planetaryescape/mxr/commit/9ca8f08a614494baf90de96826bf2e3db97a5166))


### Bug Fixes

* browser shortcuts, Messages previews, Reading duplicates, failure toasts ([#518](https://github.com/planetaryescape/mxr/issues/518)) ([9b08415](https://github.com/planetaryescape/mxr/commit/9b08415364e41d116aa8b55e3f1550474ccc1d90))

## [0.6.106](https://github.com/planetaryescape/mxr/compare/v0.6.105...v0.6.106) (2026-10-17)


### Features

* a quiet rail, a phone More sheet and one settings page (W5) ([#497](https://github.com/planetaryescape/mxr/issues/497)) ([fb23ab5](https://github.com/planetaryescape/mxr/commit/fb23ab5aa61231a8241f98b869154af4f16e20e6))
* every mode on the same rows, keys in one place (W3) ([#496](https://github.com/planetaryescape/mxr/issues/496)) ([5d9bd73](https://github.com/planetaryescape/mxr/commit/5d9bd7397f64ccb36a5cbf611fc62b3900d332af))
* finishing feels good (W8b) ([#507](https://github.com/planetaryescape/mxr/issues/507)) ([77d63c6](https://github.com/planetaryescape/mxr/commit/77d63c649a07fa18f8c271f3e188c43a48ac5e3c))
* generate web themes from base, accent and contrast ([#503](https://github.com/planetaryescape/mxr/issues/503)) ([0940a39](https://github.com/planetaryescape/mxr/commit/0940a397649f0170d8bc5c36b01f5596eb083ed0))
* offer the web app as an installable window, document extensions, guard reserved keys ([#505](https://github.com/planetaryescape/mxr/issues/505)) ([6796ccc](https://github.com/planetaryescape/mxr/commit/6796ccc8c4de72642d1cdc68e1a1762716c889a5))
* one empty, loading and error state for every page (W4b) ([#504](https://github.com/planetaryescape/mxr/issues/504)) ([0e112cb](https://github.com/planetaryescape/mxr/commit/0e112cb728ead7f59f104a2f9466f4d39dcbda22))
* one lit row and mode marks on Now and Messages (W2) ([#495](https://github.com/planetaryescape/mxr/issues/495)) ([f8d919e](https://github.com/planetaryescape/mxr/commit/f8d919ef4d502955d2e9e02b44861a9bc5b04a7c))
* one page frame for every page, role tokens and the square shape rule ([#493](https://github.com/planetaryescape/mxr/issues/493)) ([e690a20](https://github.com/planetaryescape/mxr/commit/e690a20dec4a84d657159848fee8ff5f5209feb3))
* signed desktop dmg on release, Homebrew cask, in-place new-message shortcut ([#515](https://github.com/planetaryescape/mxr/issues/515)) ([03ff2ab](https://github.com/planetaryescape/mxr/commit/03ff2ab18addcdd8610b7ce3cae679634262c227))
* the assistant as a copy desk (W7b) ([#500](https://github.com/planetaryescape/mxr/issues/500)) ([2b767ed](https://github.com/planetaryescape/mxr/commit/2b767edf86b17750e864bd1c4ad28dd95cb9012c))
* the daemon words assistant steps, changes and failures in plain language ([#494](https://github.com/planetaryescape/mxr/issues/494)) ([d6b9b31](https://github.com/planetaryescape/mxr/commit/d6b9b31aa6a0512c5314a6d4ab01ab38320e5b78))
* the email is the page when I read and the editor is the page when I write (W6) ([#498](https://github.com/planetaryescape/mxr/issues/498)) ([3b324c0](https://github.com/planetaryescape/mxr/commit/3b324c0127c68dce1a8de3a262a3c81d106bf7a5))
* thin Tauri desktop app for macOS (D1, unsigned) ([#511](https://github.com/planetaryescape/mxr/issues/511)) ([9a8d910](https://github.com/planetaryescape/mxr/commit/9a8d91021e237c9acd9b6f64d3dc53dbb8dc65ce))
* W8c design gate, graded, with the fixes it found ([#509](https://github.com/planetaryescape/mxr/issues/509)) ([2411a31](https://github.com/planetaryescape/mxr/commit/2411a315b6fd10b3f90ad30355ac632898fef8a8))


### Bug Fixes

* date the demo's Updates mail against the daemon's digest cuts ([#499](https://github.com/planetaryescape/mxr/issues/499)) ([af7481b](https://github.com/planetaryescape/mxr/commit/af7481b4f3107c2f8a77a848b51c7fb99353cbac))
* keep each web page's selection in its URL ([#489](https://github.com/planetaryescape/mxr/issues/489)) ([f209d84](https://github.com/planetaryescape/mxr/commit/f209d84ef55a2c2e582b48b92c1066f7d8335796))
* let mxr web use the daemon's bridge when it wins the port race ([#487](https://github.com/planetaryescape/mxr/issues/487)) ([ad4b1f8](https://github.com/planetaryescape/mxr/commit/ad4b1f80cc2b0f0133b70e70c763577cc686533b))
* move zip to 9 for the RUSTSEC-2026-0367/0368/0369 extraction advisories ([#513](https://github.com/planetaryescape/mxr/issues/513)) ([64b1e21](https://github.com/planetaryescape/mxr/commit/64b1e215c459352037d91ec50c4a0ca31ae97bb6))
* one definition of who is waiting on you ([#501](https://github.com/planetaryescape/mxr/issues/501)) ([1e0bbc3](https://github.com/planetaryescape/mxr/commit/1e0bbc3c576da987319a7ccb6bfb05b4abea0a65))
* open the assistant's links in the TUI and name failed steps under replies ([#490](https://github.com/planetaryescape/mxr/issues/490)) ([a669d2f](https://github.com/planetaryescape/mxr/commit/a669d2f3aef6b65e0243a2fcfb7664aaeafc7670))
* start Reading's toolbar where its title starts, on one row ([#502](https://github.com/planetaryescape/mxr/issues/502)) ([505db90](https://github.com/planetaryescape/mxr/commit/505db90ecc6fc7496b257dc199792d4b0ebb2b4c))
* window the Messages list so a lamp move costs the rows that change ([#508](https://github.com/planetaryescape/mxr/issues/508)) ([f3260a6](https://github.com/planetaryescape/mxr/commit/f3260a6ed76842c60896a2b2665cdda68cc795a4))


### Documentation

* design the web app as one product, the night desk ([#491](https://github.com/planetaryescape/mxr/issues/491)) ([7899447](https://github.com/planetaryescape/mxr/commit/78994472e98eb734e5ef0139315718bdffbfd9cf))
* keep lit rows the same height and make the Light lamp visible ([#492](https://github.com/planetaryescape/mxr/issues/492)) ([831d8d2](https://github.com/planetaryescape/mxr/commit/831d8d29d5aecad412b22958fbbb525f20ef4944))

## [0.6.105](https://github.com/planetaryescape/mxr/compare/v0.6.104...v0.6.105) (2026-10-15)


### Features

* show what each assistant step found and link the emails replies name ([#485](https://github.com/planetaryescape/mxr/issues/485)) ([46441d1](https://github.com/planetaryescape/mxr/commit/46441d1528b22b681cf07f8db5d13276a192e8a6))

## [0.6.104](https://github.com/planetaryescape/mxr/compare/v0.6.103...v0.6.104) (2026-10-15)


### Features

* an assistant you can talk to about your mail ([#476](https://github.com/planetaryescape/mxr/issues/476)) ([1e326a2](https://github.com/planetaryescape/mxr/commit/1e326a21031b65b0ab1255aae224085a8d32deef))


### Documentation

* close out the Reading x Marginalia work ([#474](https://github.com/planetaryescape/mxr/issues/474)) ([dc1b4da](https://github.com/planetaryescape/mxr/commit/dc1b4dac98893fee93f023dc9cae660c50ba34c8))
* fix Reading drift and stale CLI flag order ([#482](https://github.com/planetaryescape/mxr/issues/482)) ([d4f36d1](https://github.com/planetaryescape/mxr/commit/d4f36d1565232ffe12f11288599c1411e8cd53f0))
* log issues found in the docs drift audit ([#483](https://github.com/planetaryescape/mxr/issues/483)) ([b72d9ff](https://github.com/planetaryescape/mxr/commit/b72d9ff6f1bc7a00348d79ff2d0485b05a98f694))
* match the web guides to the calmer rows, the Archive ledger and the header Let go of all ([#478](https://github.com/planetaryescape/mxr/issues/478)) ([cc35e45](https://github.com/planetaryescape/mxr/commit/cc35e453727ff8b58a2fb0aaf1210b682b6289ba))
* say a failing Marginalia read-back waits instead of holding up others ([#477](https://github.com/planetaryescape/mxr/issues/477)) ([ec93dc3](https://github.com/planetaryescape/mxr/commit/ec93dc32f59d8be4e8c9cc800912989f77120e34))
* sweep drift from v0.6.97 to v0.6.103 ([#479](https://github.com/planetaryescape/mxr/issues/479)) ([c806fec](https://github.com/planetaryescape/mxr/commit/c806fec34a399702f1b500238a8a1f27c27a2eba))

## [0.6.103](https://github.com/planetaryescape/mxr/compare/v0.6.102...v0.6.103) (2026-10-15)


### Features

* mark a Reading highlight where you made it, not its first match ([#473](https://github.com/planetaryescape/mxr/issues/473)) ([56bccf3](https://github.com/planetaryescape/mxr/commit/56bccf3f52f2765de6c532448729b5ece3f7cff5))
* settle avatars, names and colour across the modes ([#475](https://github.com/planetaryescape/mxr/issues/475)) ([e82f358](https://github.com/planetaryescape/mxr/commit/e82f3587e8400ab19e772a7dcddb619619c60465))


### Bug Fixes

* keep mxr's code font in the reader ([#470](https://github.com/planetaryescape/mxr/issues/470)) ([db36883](https://github.com/planetaryescape/mxr/commit/db368832b9240678ab8a568d7ce628ae78b8287b))

## [0.6.102](https://github.com/planetaryescape/mxr/compare/v0.6.101...v0.6.102) (2026-10-14)


### Features

* calm the other modes' rows and use one chip ([#468](https://github.com/planetaryescape/mxr/issues/468)) ([5871b39](https://github.com/planetaryescape/mxr/commit/5871b391e830ae3f9f6ae7946016196818409e4a))


### Documentation

* record BK's waiver of the Reading cross-model review ([#469](https://github.com/planetaryescape/mxr/issues/469)) ([ceb9b60](https://github.com/planetaryescape/mxr/commit/ceb9b6060194edb79ff6cf443ff67a91be78f240))

## [0.6.101](https://github.com/planetaryescape/mxr/compare/v0.6.100...v0.6.101) (2026-10-14)


### Features

* show what an Updates email says, with number-checked gists and folded repeat alerts ([#462](https://github.com/planetaryescape/mxr/issues/462)) ([d35a602](https://github.com/planetaryescape/mxr/commit/d35a602f2d83ee4a8ccea85b420a07440dbeee74))


### Bug Fixes

* keep Marginalia highlight read-back moving past failing items ([#466](https://github.com/planetaryescape/mxr/issues/466)) ([5cd5d1e](https://github.com/planetaryescape/mxr/commit/5cd5d1e75666517327bbb3bd3aa50d9f696605b1))


### Documentation

* close out Reading rungs 1-3 and log a flaky mailbox test ([#464](https://github.com/planetaryescape/mxr/issues/464)) ([cd434e4](https://github.com/planetaryescape/mxr/commit/cd434e48f08d5c905f805d475de19472ab371ff0))

## [0.6.100](https://github.com/planetaryescape/mxr/compare/v0.6.99...v0.6.100) (2026-10-14)


### Features

* edit and remove Reading highlights in every client ([#461](https://github.com/planetaryescape/mxr/issues/461)) ([5d359cc](https://github.com/planetaryescape/mxr/commit/5d359cc811e6298b151fec4b932ddf0e23fbcfe8))

## [0.6.99](https://github.com/planetaryescape/mxr/compare/v0.6.98...v0.6.99) (2026-10-14)


### Features

* route Claude models through Anthropic first with OpenRouter fallback ([#458](https://github.com/planetaryescape/mxr/issues/458)) ([adfad63](https://github.com/planetaryescape/mxr/commit/adfad63c170fb4a29ebff6dd91a5676fa738ef8c))


### Bug Fixes

* update rustls to 0.23.46 for RUSTSEC-2026-0346 to 0349 ([#460](https://github.com/planetaryescape/mxr/issues/460)) ([dabed70](https://github.com/planetaryescape/mxr/commit/dabed707082fbd9a48c614d024d9a98f8472ca88))

## [0.6.98](https://github.com/planetaryescape/mxr/compare/v0.6.97...v0.6.98) (2026-10-14)


### Features

* calm Updates into a notification reel ([#454](https://github.com/planetaryescape/mxr/issues/454)) ([b825001](https://github.com/planetaryescape/mxr/commit/b82500172829d599415fc3d0ed92db647d3e989d))
* give every mode one header, heading and type scale ([#452](https://github.com/planetaryescape/mxr/issues/452)) ([795c0d1](https://github.com/planetaryescape/mxr/commit/795c0d1b0fb827257c17fe4149ef3ffc378b84cb))
* render Reading articles from their HTML with reader themes and settings ([#457](https://github.com/planetaryescape/mxr/issues/457)) ([1c87564](https://github.com/planetaryescape/mxr/commit/1c87564bafc4a6c667d88f2288c38d4e2288f3f3))


### Documentation

* add the reading CLI reference and close out the Marginalia handoff ([#455](https://github.com/planetaryescape/mxr/issues/455)) ([04f564f](https://github.com/planetaryescape/mxr/commit/04f564f0a7b6f6bd2e583a6f7e9159e57704252c))

## [0.6.97](https://github.com/planetaryescape/mxr/compare/v0.6.96...v0.6.97) (2026-10-14)


### Features

* open Reading items in Marginalia and read their highlights back ([#447](https://github.com/planetaryescape/mxr/issues/447)) ([3262b30](https://github.com/planetaryescape/mxr/commit/3262b304adcc7da9dacc1a9ccc2340b52b93e851))

## [0.6.96](https://github.com/planetaryescape/mxr/compare/v0.6.95...v0.6.96) (2026-10-13)


### Features

* enable host-approved remote replies and send recovery ([#450](https://github.com/planetaryescape/mxr/issues/450)) ([f4d02b6](https://github.com/planetaryescape/mxr/commit/f4d02b6405cf7414e457004e24d870056a303c0f))


### Documentation

* close out remote mailbox release ([#449](https://github.com/planetaryescape/mxr/issues/449)) ([2508dcf](https://github.com/planetaryescape/mxr/commit/2508dcfefb412f25e74fb934170fa332e4eb0490))

## [0.6.95](https://github.com/planetaryescape/mxr/compare/v0.6.94...v0.6.95) (2026-10-13)


### Features

* read a paired remote mailbox from native clients ([#446](https://github.com/planetaryescape/mxr/issues/446)) ([b2ceb47](https://github.com/planetaryescape/mxr/commit/b2ceb47ffbe0ea8839384d94945ed28810e95f04))


### Documentation

* record released pairing authority foundation ([#445](https://github.com/planetaryescape/mxr/issues/445)) ([4b2a544](https://github.com/planetaryescape/mxr/commit/4b2a5443cbb43128fadf8f2cc7a90b3d1aee1ccf))

## [0.6.94](https://github.com/planetaryescape/mxr/compare/v0.6.93...v0.6.94) (2026-10-12)


### Features

* establish revocable local pairing authority ([#443](https://github.com/planetaryescape/mxr/issues/443)) ([974ec3d](https://github.com/planetaryescape/mxr/commit/974ec3d54312e283504fd138e014392b60be6082))


### Documentation

* align guides with shipped modes and task workflows ([c0a4b11](https://github.com/planetaryescape/mxr/commit/c0a4b11553088e3c95326b2cb69d10fb8c1979fd))
* verify examples and preserve background-work contracts ([bf66f45](https://github.com/planetaryescape/mxr/commit/bf66f451ac1d6ac9463290844926c9626a942cea))

## [0.6.93](https://github.com/planetaryescape/mxr/compare/v0.6.92...v0.6.93) (2026-10-12)


### Bug Fixes

* bound background to-do resolution checks ([#437](https://github.com/planetaryescape/mxr/issues/437)) ([c716405](https://github.com/planetaryescape/mxr/commit/c716405809a42d36f86773bfd7128dec857fdda6))


### Documentation

* publish backup recovery guide and readiness acceptance ([#439](https://github.com/planetaryescape/mxr/issues/439)) ([30d374b](https://github.com/planetaryescape/mxr/commit/30d374b982546ebe112e4bc16d67a7a5894f925c))

## [0.6.92](https://github.com/planetaryescape/mxr/compare/v0.6.91...v0.6.92) (2026-10-12)


### Features

* restore portable backups with old work held ([#436](https://github.com/planetaryescape/mxr/issues/436)) ([8faa88f](https://github.com/planetaryescape/mxr/commit/8faa88f1c11ec0f66eb4f37c5212189b00f74aa7))


### Documentation

* record published selected-client acceptance ([#435](https://github.com/planetaryescape/mxr/issues/435)) ([0ac274f](https://github.com/planetaryescape/mxr/commit/0ac274f627ba0fc5e833916112c9a065bb402d49))

## [0.6.91](https://github.com/planetaryescape/mxr/compare/v0.6.90...v0.6.91) (2026-10-12)


### Features

* use the selected daemon across CLI TUI and MCP ([#433](https://github.com/planetaryescape/mxr/issues/433)) ([9330c48](https://github.com/planetaryescape/mxr/commit/9330c48e878e9f9926d69f066edeef61c4ace144))


### Documentation

* document workspace ownership and published recovery checks ([#431](https://github.com/planetaryescape/mxr/issues/431)) ([4f965b7](https://github.com/planetaryescape/mxr/commit/4f965b78f95fe2a21affbc2d2d1cde3e1c8bb481))
* retain workspace owner in send recovery examples ([#432](https://github.com/planetaryescape/mxr/issues/432)) ([b7637cb](https://github.com/planetaryescape/mxr/commit/b7637cb688539bd58290418fe0c5005e75fe64dc))

## [0.6.90](https://github.com/planetaryescape/mxr/compare/v0.6.89...v0.6.90) (2026-10-12)


### Bug Fixes

* preserve protocol and MCP compatibility with dependency updates ([#430](https://github.com/planetaryescape/mxr/issues/430)) ([3698882](https://github.com/planetaryescape/mxr/commit/36988825d14f9f08cb3a2ccaf7c607559e7e317d))


### Documentation

* clarify email evidence and current provider state ([#428](https://github.com/planetaryescape/mxr/issues/428)) ([e676c8a](https://github.com/planetaryescape/mxr/commit/e676c8add9dae577d890d9425bcfe1d2675780bd))

## [0.6.89](https://github.com/planetaryescape/mxr/compare/v0.6.88...v0.6.89) (2026-10-12)


### Bug Fixes

* retain reviewed workspace in release smoke proofs ([#425](https://github.com/planetaryescape/mxr/issues/425)) ([17a0df2](https://github.com/planetaryescape/mxr/commit/17a0df2c8e0f61f84810086c4d5a03cf0e0e034e))

## [0.6.88](https://github.com/planetaryescape/mxr/compare/v0.6.87...v0.6.88) (2026-10-12)


### Bug Fixes

* keep private pending work bound to its original workspace ([#422](https://github.com/planetaryescape/mxr/issues/422)) ([45f917b](https://github.com/planetaryescape/mxr/commit/45f917b53e26cef3b9fdde8ff3fad025084d15e7))

## [0.6.87](https://github.com/planetaryescape/mxr/compare/v0.6.86...v0.6.87) (2026-10-12)


### Features

* assign personal categories with reviewed rules ([#418](https://github.com/planetaryescape/mxr/issues/418)) ([7af9e32](https://github.com/planetaryescape/mxr/commit/7af9e325c93d2d11f46d4120cc0b43e7d2ecf3d7))


### Documentation

* record published draft attachment acceptance ([#420](https://github.com/planetaryescape/mxr/issues/420)) ([1f7e21b](https://github.com/planetaryescape/mxr/commit/1f7e21b29f0761f77090f9e3928c4e7e7da35a3c))

## [0.6.86](https://github.com/planetaryescape/mxr/compare/v0.6.85...v0.6.86) (2026-10-12)


### Features

* retain immutable draft attachments across clients ([#413](https://github.com/planetaryescape/mxr/issues/413)) ([6d6fe2d](https://github.com/planetaryescape/mxr/commit/6d6fe2da39178681de396f8f807d4d7bc3020961))

## [0.6.85](https://github.com/planetaryescape/mxr/compare/v0.6.84...v0.6.85) (2026-10-12)


### Features

* offer evidenced to-do resolutions and date changes ([#414](https://github.com/planetaryescape/mxr/issues/414)) ([32cd1fe](https://github.com/planetaryescape/mxr/commit/32cd1feb5f8cafdf003c6426c897fbda395c002e))


### Documentation

* record headless release acceptance ([#412](https://github.com/planetaryescape/mxr/issues/412)) ([525bde7](https://github.com/planetaryescape/mxr/commit/525bde74bfea87119bb3dd99ef3a11aafaee3943))

## [0.6.84](https://github.com/planetaryescape/mxr/compare/v0.6.83...v0.6.84) (2026-10-12)


### Features

* expose supervised daemon and model readiness ([#398](https://github.com/planetaryescape/mxr/issues/398)) ([b6b88de](https://github.com/planetaryescape/mxr/commit/b6b88deff57e0ab1b6f523844e1708e63d2121a7))

## [0.6.83](https://github.com/planetaryescape/mxr/compare/v0.6.82...v0.6.83) (2026-10-12)


### Features

* add personal categories across mail clients ([#406](https://github.com/planetaryescape/mxr/issues/406)) ([ef677cc](https://github.com/planetaryescape/mxr/commit/ef677ccda4b62d65aac98310197e79e69ce3fdf4))


### Documentation

* close released browser recovery and attachment transfers ([#407](https://github.com/planetaryescape/mxr/issues/407)) ([12109ea](https://github.com/planetaryescape/mxr/commit/12109eac12265b8e1e2567c49cdb7289b44c69ce))

## [0.6.82](https://github.com/planetaryescape/mxr/compare/v0.6.81...v0.6.82) (2026-10-12)


### Bug Fixes

* create missing directories for attachment downloads ([#404](https://github.com/planetaryescape/mxr/issues/404)) ([9c5c306](https://github.com/planetaryescape/mxr/commit/9c5c306adda575ec622694eb53ec72fa97f09b83))

## [0.6.81](https://github.com/planetaryescape/mxr/compare/v0.6.80...v0.6.81) (2026-10-11)


### Bug Fixes

* restore keyboard Help focus and repair stale CI configuration ([#401](https://github.com/planetaryescape/mxr/issues/401)) ([55ff4fb](https://github.com/planetaryescape/mxr/commit/55ff4fb8eb5fb93911f098b057c7bc87552e53a6))

## [0.6.80](https://github.com/planetaryescape/mxr/compare/v0.6.79...v0.6.80) (2026-10-11)


### Features

* transfer attachments to consuming clients ([#395](https://github.com/planetaryescape/mxr/issues/395)) ([4705e75](https://github.com/planetaryescape/mxr/commit/4705e7584ca267e18ff3447a16fe3b1046c17ce7))


### Documentation

* record released browser event recovery ([#396](https://github.com/planetaryescape/mxr/issues/396)) ([b45ebd5](https://github.com/planetaryescape/mxr/commit/b45ebd578aad2bc8f31957d892789174d63a60b5))

## [0.6.79](https://github.com/planetaryescape/mxr/compare/v0.6.78...v0.6.79) (2026-10-11)


### Features

* create review tasks for upcoming annual renewals ([#392](https://github.com/planetaryescape/mxr/issues/392)) ([5fa69b3](https://github.com/planetaryescape/mxr/commit/5fa69b3290f2a10af9b5c475f0f69568bcc89bdc))


### Bug Fixes

* refresh mailbox queries after event reconnection ([#391](https://github.com/planetaryescape/mxr/issues/391)) ([d9ceba7](https://github.com/planetaryescape/mxr/commit/d9ceba71a8aa17de9acc0b50d3067e123bcfdb85))

## [0.6.78](https://github.com/planetaryescape/mxr/compare/v0.6.77...v0.6.78) (2026-10-11)


### Bug Fixes

* polish contrast and expire idle terminal status ([#388](https://github.com/planetaryescape/mxr/issues/388)) ([4f736ec](https://github.com/planetaryescape/mxr/commit/4f736ec5b28e6ac151ad354dbee32557322b538e))


### Documentation

* publish Archive recovery guides and release evidence ([#387](https://github.com/planetaryescape/mxr/issues/387)) ([f2e522c](https://github.com/planetaryescape/mxr/commit/f2e522c882599127d6fae9fbe5a0855f542472b4))

## [0.6.77](https://github.com/planetaryescape/mxr/compare/v0.6.76...v0.6.77) (2026-10-11)


### Features

* recover reviewed Archive outcomes across clients ([#384](https://github.com/planetaryescape/mxr/issues/384)) ([0153a25](https://github.com/planetaryescape/mxr/commit/0153a25a967710c9ea6e86349f7d97d42dc8088e))

## [0.6.76](https://github.com/planetaryescape/mxr/compare/v0.6.75...v0.6.76) (2026-10-11)


### Bug Fixes

* identify account in sweep confirmation ([#380](https://github.com/planetaryescape/mxr/issues/380)) ([1397f4b](https://github.com/planetaryescape/mxr/commit/1397f4b6dc7f4965df547eebf715bd10756e5d80))

## [0.6.75](https://github.com/planetaryescape/mxr/compare/v0.6.74...v0.6.75) (2026-10-11)


### Features

* sync all enabled accounts across clients ([#353](https://github.com/planetaryescape/mxr/issues/353)) ([e7006ee](https://github.com/planetaryescape/mxr/commit/e7006ee47cd42f18e193950b4d076a9f9110abd7))

## [0.6.74](https://github.com/planetaryescape/mxr/compare/v0.6.73...v0.6.74) (2026-10-11)


### Bug Fixes

* clear completed mutation progress status ([#376](https://github.com/planetaryescape/mxr/issues/376)) ([6f2defc](https://github.com/planetaryescape/mxr/commit/6f2defccbfefb1d90b3645714a16c8d2031110ce))

## [0.6.73](https://github.com/planetaryescape/mxr/compare/v0.6.72...v0.6.73) (2026-10-11)


### Bug Fixes

* preserve inline answers and forwarded message context ([#348](https://github.com/planetaryescape/mxr/issues/348)) ([299bed2](https://github.com/planetaryescape/mxr/commit/299bed27b2189032b0efadaca1bf18416f0b387f))

## [0.6.72](https://github.com/planetaryescape/mxr/compare/v0.6.71...v0.6.72) (2026-10-11)


### Features

* persist Updates pins and add terminal pin and sweep actions ([#354](https://github.com/planetaryescape/mxr/issues/354)) ([9881acc](https://github.com/planetaryescape/mxr/commit/9881acca60ec7a26072338435212da4b907d4c55))


### Documentation

* explain reader appearance choices ([#371](https://github.com/planetaryescape/mxr/issues/371)) ([caae87c](https://github.com/planetaryescape/mxr/commit/caae87ca1aa254cafa235351cd963ecab2a81c1a))

## [0.6.71](https://github.com/planetaryescape/mxr/compare/v0.6.70...v0.6.71) (2026-10-11)


### Bug Fixes

* quiet repeated sync failures and clarify account status ([#352](https://github.com/planetaryescape/mxr/issues/352)) ([a5cce8b](https://github.com/planetaryescape/mxr/commit/a5cce8bbce0cbe400cc8482784e1fa809726c975))

## [0.6.70](https://github.com/planetaryescape/mxr/compare/v0.6.69...v0.6.70) (2026-10-11)


### Features

* add comfortable automatic email canvases ([#349](https://github.com/planetaryescape/mxr/issues/349)) ([dbab130](https://github.com/planetaryescape/mxr/commit/dbab13027822d8b46576f17bb215c61d8f2d305d))


### Bug Fixes

* keep paused cold mail loads out of empty state ([#366](https://github.com/planetaryescape/mxr/issues/366)) ([75678e7](https://github.com/planetaryescape/mxr/commit/75678e7d4c4ea57408d43a5620df0150e8e0693a))


### Documentation

* clarify web app help and recovery ([#360](https://github.com/planetaryescape/mxr/issues/360)) ([5e284b0](https://github.com/planetaryescape/mxr/commit/5e284b00b147b0c5c97bc3790aeb11574307e585))
* close out published send recovery readiness ([#362](https://github.com/planetaryescape/mxr/issues/362)) ([49e40f0](https://github.com/planetaryescape/mxr/commit/49e40f036ad83d68e09cf3d9de6e016d877d6ccc))

## [0.6.69](https://github.com/planetaryescape/mxr/compare/v0.6.68...v0.6.69) (2026-10-11)


### Bug Fixes

* improve mailbox spacing and setup recovery ([#351](https://github.com/planetaryescape/mxr/issues/351)) ([35c4f72](https://github.com/planetaryescape/mxr/commit/35c4f7232125cd595bc2169abbf951152a7aa06d))

## [0.6.68](https://github.com/planetaryescape/mxr/compare/v0.6.67...v0.6.68) (2026-10-11)


### Bug Fixes

* align release smoke target directory ([#357](https://github.com/planetaryescape/mxr/issues/357)) ([86c9599](https://github.com/planetaryescape/mxr/commit/86c959921c7a7af16ad2f670411a404198e95c72))

## [0.6.67](https://github.com/planetaryescape/mxr/compare/v0.6.66...v0.6.67) (2026-10-11)


### Bug Fixes

* retain reviewed send outcomes across disconnects ([#347](https://github.com/planetaryescape/mxr/issues/347)) ([d6ba93b](https://github.com/planetaryescape/mxr/commit/d6ba93ba7082a644e027b331e67da44d8e6dd4e8))


### Documentation

* record installed daemon portability acceptance ([#346](https://github.com/planetaryescape/mxr/issues/346)) ([f1e19fb](https://github.com/planetaryescape/mxr/commit/f1e19fba2055fa96bbaf73ce35a6bd8354cd4870))

## [0.6.66](https://github.com/planetaryescape/mxr/compare/v0.6.65...v0.6.66) (2026-10-11)


### Features

* improve package status in Updates ([#343](https://github.com/planetaryescape/mxr/issues/343)) ([8c8d069](https://github.com/planetaryescape/mxr/commit/8c8d069d81faf4603075c65afc5246dc33bf79fd))

## [0.6.65](https://github.com/planetaryescape/mxr/compare/v0.6.64...v0.6.65) (2026-10-11)


### Features

* preserve draft revisions across clients ([#341](https://github.com/planetaryescape/mxr/issues/341)) ([a631583](https://github.com/planetaryescape/mxr/commit/a631583c8fd8438abc6892fdf10fb49dc09e3748))

## [0.6.64](https://github.com/planetaryescape/mxr/compare/v0.6.63...v0.6.64) (2026-10-11)


### Bug Fixes

* stop the daemon cleanly under process supervision ([#338](https://github.com/planetaryescape/mxr/issues/338)) ([01b5ab5](https://github.com/planetaryescape/mxr/commit/01b5ab5285473fc5af854c68fcd8e214cd056931))


### Documentation

* record portability releases and follow-up audits ([#337](https://github.com/planetaryescape/mxr/issues/337)) ([8434fb9](https://github.com/planetaryescape/mxr/commit/8434fb9ccf8ee0bd833205ff78a7538d56c0f873))

## [0.6.63](https://github.com/planetaryescape/mxr/compare/v0.6.62...v0.6.63) (2026-10-10)


### Bug Fixes

* atomically persist the Gmail token cache ([#333](https://github.com/planetaryescape/mxr/issues/333)) ([3d17b16](https://github.com/planetaryescape/mxr/commit/3d17b1600a9db36b9f062bc4e2ca6997d8a9c562))

## [0.6.62](https://github.com/planetaryescape/mxr/compare/v0.6.61...v0.6.62) (2026-10-10)


### Bug Fixes

* diagnose the selected daemon without local fallback ([#331](https://github.com/planetaryescape/mxr/issues/331)) ([f783d93](https://github.com/planetaryescape/mxr/commit/f783d937b3dbabb23c338f0164d96c0b875b4c89))

## [0.6.61](https://github.com/planetaryescape/mxr/compare/v0.6.60...v0.6.61) (2026-10-09)


### Bug Fixes

* calm Now, require explicit Messages opening and clarify To do ([4b4b524](https://github.com/planetaryescape/mxr/commit/4b4b52452e4b146e74313f7fabd6e82c5edf2dee))


### Documentation

* correct sorting rule account ID lookup ([0b00f84](https://github.com/planetaryescape/mxr/commit/0b00f847c073386f154cd7e36aacfe75b8dc4f7d))

## [0.6.60](https://github.com/planetaryescape/mxr/compare/v0.6.59...v0.6.60) (2026-10-09)


### Features

* add account-scoped mail sorting rules ([88afd25](https://github.com/planetaryescape/mxr/commit/88afd251be7ddabbc8e19d5fb07815f32b971c83))

## [0.6.59](https://github.com/planetaryescape/mxr/compare/v0.6.58...v0.6.59) (2026-10-09)


### Features

* Archive finds subscriptions in your receipts and invoices ([#315](https://github.com/planetaryescape/mxr/issues/315)) ([57b39c7](https://github.com/planetaryescape/mxr/commit/57b39c7afa1cacf4092e30658b631e6ace36257d))
* Midnight dark theme with a middle text tier ([#321](https://github.com/planetaryescape/mxr/issues/321)) ([68e57a1](https://github.com/planetaryescape/mxr/commit/68e57a19a07d811f4e682be8f11749d6625e8b59))
* serve Updates as a twice-daily briefing by source ([#313](https://github.com/planetaryescape/mxr/issues/313)) ([f72f89c](https://github.com/planetaryescape/mxr/commit/f72f89cc138348a11362f150016089a10461120d))
* trust-reclassify — arrivals line, Not sure, and X/K mode moves ([#314](https://github.com/planetaryescape/mxr/issues/314)) ([16b9dfc](https://github.com/planetaryescape/mxr/commit/16b9dfc611a0ea1009cad408eb42c08df3586ecf))


### Bug Fixes

* Reading unsubscribe offers just unsubscribe or unsubscribe and clear ([#319](https://github.com/planetaryescape/mxr/issues/319)) ([1fa3ade](https://github.com/planetaryescape/mxr/commit/1fa3ade15bd6398f7c2a40e89f6587f498e27db5))
* sweep and pin are available on the Updates page ([#323](https://github.com/planetaryescape/mxr/issues/323)) ([26b09bf](https://github.com/planetaryescape/mxr/commit/26b09bf9b7bd03eb44a154a2f1b492474453adf3))
* web app shows a not-found page, a no-account frame and labelled IMAP fields ([#322](https://github.com/planetaryescape/mxr/issues/322)) ([939bef1](https://github.com/planetaryescape/mxr/commit/939bef10add27259eabf0ebc4b87732f2bc53d42))

## [0.6.58](https://github.com/planetaryescape/mxr/compare/v0.6.57...v0.6.58) (2026-10-08)


### Features

* Reading is an edition you visit, not a pile you owe ([#309](https://github.com/planetaryescape/mxr/issues/309)) ([a95d86a](https://github.com/planetaryescape/mxr/commit/a95d86a536ee351257df1232efb6a980f4cb0643))
* show how fresh your mail is on every page ([#302](https://github.com/planetaryescape/mxr/issues/302)) ([83ef39a](https://github.com/planetaryescape/mxr/commit/83ef39ac1c5469f6cefd5ca2e962a14b14a64fca))


### Bug Fixes

* count a message's paragraphs by its own line endings, not just \n\n ([#312](https://github.com/planetaryescape/mxr/issues/312)) ([562f16b](https://github.com/planetaryescape/mxr/commit/562f16b30ae2646b220f7220d68ec3e09171d753))
* keep split panes mounted so opening a conversation is fast again ([#306](https://github.com/planetaryescape/mxr/issues/306)) ([fb6c3d4](https://github.com/planetaryescape/mxr/commit/fb6c3d4ea0c30fec92040873569c4c6fd222e4a2))
* list every Archive match when a query only names something ([#307](https://github.com/planetaryescape/mxr/issues/307)) ([36943b6](https://github.com/planetaryescape/mxr/commit/36943b6e4298e8e730d8db3766b94d959ecdac09))
* promises only from mail you sent; machine-like senders out of Messages ([#308](https://github.com/planetaryescape/mxr/issues/308)) ([cc88474](https://github.com/planetaryescape/mxr/commit/cc884740297ac1c09c113650b92b0404627a90ea))
* replace top-of-page teaching cards with contextual hints ([#304](https://github.com/planetaryescape/mxr/issues/304)) ([f65bbc2](https://github.com/planetaryescape/mxr/commit/f65bbc2b5d8b67353f4b36bca4cf1aa0cad93e4e))
* show when the email arrived on each To do row ([#311](https://github.com/planetaryescape/mxr/issues/311)) ([1d6b91b](https://github.com/planetaryescape/mxr/commit/1d6b91b6d5232663db53aff995d2ac5c7f619496))

## [0.6.57](https://github.com/planetaryescape/mxr/compare/v0.6.56...v0.6.57) (2026-10-07)


### Bug Fixes

* done here moves on visibly, with coloured top-centre toasts ([#301](https://github.com/planetaryescape/mxr/issues/301)) ([954c3f0](https://github.com/planetaryescape/mxr/commit/954c3f0cab44589304c6a1c82d79e6290ce8ff6f))
* make the sidebar and every list/detail split resizable ([#299](https://github.com/planetaryescape/mxr/issues/299)) ([9e5aae8](https://github.com/planetaryescape/mxr/commit/9e5aae8d8cc39b233bc7085bcb0fb0ef9a53a097))
* scoped agents can only see and change their allowed accounts ([#303](https://github.com/planetaryescape/mxr/issues/303)) ([d82932a](https://github.com/planetaryescape/mxr/commit/d82932a76c2bc06bda8706a074dd1c1d8a8802b7))

## [0.6.56](https://github.com/planetaryescape/mxr/compare/v0.6.55...v0.6.56) (2026-10-07)


### Bug Fixes

* use wide screens with one shared page frame ([#297](https://github.com/planetaryescape/mxr/issues/297)) ([5a15b00](https://github.com/planetaryescape/mxr/commit/5a15b001babf0d9171ede32d10858a3f0611915d))


### Documentation

* sorting shows its work, so nothing feels hidden ([#296](https://github.com/planetaryescape/mxr/issues/296)) ([735e60b](https://github.com/planetaryescape/mxr/commit/735e60b574d6294ccabb456857fad4f59b55a87d))

## [0.6.55](https://github.com/planetaryescape/mxr/compare/v0.6.54...v0.6.55) (2026-10-07)


### Features

* Archive keeps records you can ask questions of ([#294](https://github.com/planetaryescape/mxr/issues/294)) ([260a299](https://github.com/planetaryescape/mxr/commit/260a2991cccaa30385f0664ad730bc40d2bc823b))

## [0.6.54](https://github.com/planetaryescape/mxr/compare/v0.6.53...v0.6.54) (2026-10-07)


### Features

* Messages shows people, with their conversations as topics ([#290](https://github.com/planetaryescape/mxr/issues/290)) ([584ed33](https://github.com/planetaryescape/mxr/commit/584ed33b85e59f95924b500f4e55dfece0a63e21))


### Bug Fixes

* clear this week's npm advisories in the web app and docs site ([#291](https://github.com/planetaryescape/mxr/issues/291)) ([871f6f7](https://github.com/planetaryescape/mxr/commit/871f6f73df37751d821b887a4b63439a7985c68e))

## [0.6.53](https://github.com/planetaryescape/mxr/compare/v0.6.52...v0.6.53) (2026-10-03)


### Features

* Now is the front page and the rail is the modes ([#288](https://github.com/planetaryescape/mxr/issues/288)) ([ad78800](https://github.com/planetaryescape/mxr/commit/ad78800f83c3b3e7a5e1419eb89a7729f20f17ff))

## [0.6.52](https://github.com/planetaryescape/mxr/compare/v0.6.51...v0.6.52) (2026-10-03)


### Features

* To do, mail that asks you to act as a runway of instructions ([#285](https://github.com/planetaryescape/mxr/issues/285)) ([8975717](https://github.com/planetaryescape/mxr/commit/89757173a3a0f8baf4d29d752ec2cf9dc1ba4669))


### Bug Fixes

* clear new npm advisories and allow one with no fix in the docs build ([#286](https://github.com/planetaryescape/mxr/issues/286)) ([6929eee](https://github.com/planetaryescape/mxr/commit/6929eee265c5865d73c81edc57218418685cec59))


### Documentation

* email is five apps at once (blueprint 22, research, docs site and READMEs) ([#273](https://github.com/planetaryescape/mxr/issues/273)) ([ec60330](https://github.com/planetaryescape/mxr/commit/ec6033032a21b88fbdc01474a1dc78666d8ad56b))
* the app teaches itself in place, with no tour ([#284](https://github.com/planetaryescape/mxr/issues/284)) ([76e22ce](https://github.com/planetaryescape/mxr/commit/76e22ce6901ac473a342dde36c416fca508ded67))

## [0.6.51](https://github.com/planetaryescape/mxr/compare/v0.6.50...v0.6.51) (2026-10-02)


### Bug Fixes

* deleting an email deletes everything derived from it ([#282](https://github.com/planetaryescape/mxr/issues/282)) ([8234399](https://github.com/planetaryescape/mxr/commit/82343998804c0581c1cdc7b4b4f6e8154d15d00e))

## [0.6.50](https://github.com/planetaryescape/mxr/compare/v0.6.49...v0.6.50) (2026-10-02)


### Bug Fixes

* compare the tagged name value in the release-please lock rule ([#281](https://github.com/planetaryescape/mxr/issues/281)) ([c02ed3c](https://github.com/planetaryescape/mxr/commit/c02ed3cd2cf64bca6515b1e897b578c69ac01e3d))
* keep release-please off the vendored hnsw_rs version ([#279](https://github.com/planetaryescape/mxr/issues/279)) ([0dfbf53](https://github.com/planetaryescape/mxr/commit/0dfbf53da08f41dd407a6151eb2e5449382e86e4))

## [0.6.49](https://github.com/planetaryescape/mxr/compare/v0.6.48...v0.6.49) (2026-10-02)


### Bug Fixes

* semantic search reaches every chunk; batch mutations reindex once ([#277](https://github.com/planetaryescape/mxr/issues/277)) ([75db2c9](https://github.com/planetaryescape/mxr/commit/75db2c92e055cd1e6884be3c5c6554a109c9c5d1))

## [0.6.48](https://github.com/planetaryescape/mxr/compare/v0.6.47...v0.6.48) (2026-10-02)


### Features

* owed equals the desk's You owe lane; Allow makes a person, not a conversation ([#275](https://github.com/planetaryescape/mxr/issues/275)) ([bc9fe58](https://github.com/planetaryescape/mxr/commit/bc9fe5814c1bbaae421c285e6b5dff7b20d60023))


### Bug Fixes

* polish from the v0.6.47 product tour ([#274](https://github.com/planetaryescape/mxr/issues/274)) ([1787925](https://github.com/planetaryescape/mxr/commit/1787925327beb7c17a630e489d80e4bdf6702c2b))

## [0.6.47](https://github.com/planetaryescape/mxr/compare/v0.6.46...v0.6.47) (2026-10-01)


### Bug Fixes

* make star and unstar act on the whole conversation and undo exactly ([#270](https://github.com/planetaryescape/mxr/issues/270)) ([580f3d4](https://github.com/planetaryescape/mxr/commit/580f3d4a29cbf2b048ecf8f6ee3fbb1df5e09813))


### Documentation

* record the second independent rubric grade and real-mailbox timings ([#269](https://github.com/planetaryescape/mxr/issues/269)) ([68fac81](https://github.com/planetaryescape/mxr/commit/68fac81691bf0673676544f4d31dff127cf88adb))

## [0.6.46](https://github.com/planetaryescape/mxr/compare/v0.6.45...v0.6.46) (2026-10-01)


### Bug Fixes

* time star and cached opens, undo every verb, and hold motion and dates to one standard ([#267](https://github.com/planetaryescape/mxr/issues/267)) ([7ec4500](https://github.com/planetaryescape/mxr/commit/7ec4500ecaab48deae1c58e978ef934182e0962c))

## [0.6.45](https://github.com/planetaryescape/mxr/compare/v0.6.44...v0.6.45) (2026-09-30)


### Features

* keep the web app usable while the daemon is stopped ([#264](https://github.com/planetaryescape/mxr/issues/264)) ([9915e61](https://github.com/planetaryescape/mxr/commit/9915e61881652068ba4b3eb1481b78914c2563fb))

## [0.6.44](https://github.com/planetaryescape/mxr/compare/v0.6.43...v0.6.44) (2026-09-30)


### Features

* defer a conversation to a time and have it come back on its own ([#261](https://github.com/planetaryescape/mxr/issues/261)) ([1e61e7a](https://github.com/planetaryescape/mxr/commit/1e61e7a13b6eeb3aee4bd71c9554af262603175c))
* show where AI drafts come from, and keep your history off cloud models ([#262](https://github.com/planetaryescape/mxr/issues/262)) ([5973dfa](https://github.com/planetaryescape/mxr/commit/5973dfa7c8b399da0fe940373c1ce42494b58946))

## [0.6.43](https://github.com/planetaryescape/mxr/compare/v0.6.42...v0.6.43) (2026-09-30)


### Bug Fixes

* answer mxr owed in a fifth of a second on large mailboxes ([#259](https://github.com/planetaryescape/mxr/issues/259)) ([8c9d371](https://github.com/planetaryescape/mxr/commit/8c9d371b2ee2d1b4b2b4bf401c35e1844779b32e))


### Documentation

* add the web experience plan to the blueprint ([#258](https://github.com/planetaryescape/mxr/issues/258)) ([531a921](https://github.com/planetaryescape/mxr/commit/531a9213a19240038276b00edac54aa2cbd9d51c))
* record the independent rubric grade ([#257](https://github.com/planetaryescape/mxr/issues/257)) ([983dd14](https://github.com/planetaryescape/mxr/commit/983dd14dadfb1bf16ae60131065cbe0481472359))

## [0.6.42](https://github.com/planetaryescape/mxr/compare/v0.6.41...v0.6.42) (2026-09-30)


### Features

* see what each email is about and what it asks, right in the list ([#255](https://github.com/planetaryescape/mxr/issues/255)) ([4e4b061](https://github.com/planetaryescape/mxr/commit/4e4b0615080c39a25f35b515ca0d6c3c0985d439))

## [0.6.41](https://github.com/planetaryescape/mxr/compare/v0.6.40...v0.6.41) (2026-09-30)


### Bug Fixes

* give the web app and the TUI the same keys ([#252](https://github.com/planetaryescape/mxr/issues/252)) ([873f0ab](https://github.com/planetaryescape/mxr/commit/873f0ab9ed9eb2206686e2d41bc4892dbbfe94e4))


### Documentation

* refresh the docs site for the desk, focus, places and Done, and make CI build it again ([#254](https://github.com/planetaryescape/mxr/issues/254)) ([231713c](https://github.com/planetaryescape/mxr/commit/231713c788a942d6d23d56566d3edd7beeb0a9e3))

## [0.6.40](https://github.com/planetaryescape/mxr/compare/v0.6.39...v0.6.40) (2026-09-28)


### Features

* put a desk item away with Done: archived, read, and gone until someone writes again ([#249](https://github.com/planetaryescape/mxr/issues/249)) ([bf01ab9](https://github.com/planetaryescape/mxr/commit/bf01ab9867161dbe82dcc076478438a9bc405ea3))

## [0.6.39](https://github.com/planetaryescape/mxr/compare/v0.6.38...v0.6.39) (2026-09-28)


### Features

* make mxr feel good: low tide, an optional sound palette, key hints, touch swipe and a typography pass ([#247](https://github.com/planetaryescape/mxr/issues/247)) ([4018a78](https://github.com/planetaryescape/mxr/commit/4018a78011dc6ebf98e1b6940fe1e1f397897c01))

## [0.6.38](https://github.com/planetaryescape/mxr/compare/v0.6.37...v0.6.38) (2026-09-27)


### Features

* give mail that isn't from people its own places: Reading and Paper trail, with pins and sweep ([#245](https://github.com/planetaryescape/mxr/issues/245)) ([4abc93c](https://github.com/planetaryescape/mxr/commit/4abc93c53e5511f8b97add6e2e22f5cd623efa89))


### Bug Fixes

* make the large-list e2e measure the app's work, not the runner's load ([#244](https://github.com/planetaryescape/mxr/issues/244)) ([2076791](https://github.com/planetaryescape/mxr/commit/2076791f8cfd366eabbdac33195e5c8d80d79186))

## [0.6.37](https://github.com/planetaryescape/mxr/compare/v0.6.36...v0.6.37) (2026-09-27)


### Features

* reply to everyone you owe in one focused sitting, and keep the promises you make ([#242](https://github.com/planetaryescape/mxr/issues/242)) ([1626116](https://github.com/planetaryescape/mxr/commit/16261168d4ef0a63b82a28ee0f540b443a3ac5b1))

## [0.6.36](https://github.com/planetaryescape/mxr/compare/v0.6.35...v0.6.36) (2026-09-27)


### Features

* open mxr on the desk: what needs you, not what arrived ([#240](https://github.com/planetaryescape/mxr/issues/240)) ([c571d58](https://github.com/planetaryescape/mxr/commit/c571d582d3f5d1155acd9e948fc648376d1e06eb))

## [0.6.35](https://github.com/planetaryescape/mxr/compare/v0.6.34...v0.6.35) (2026-09-27)


### Features

* open a conversation to its context: the ask, your promises, and how you know them ([#238](https://github.com/planetaryescape/mxr/issues/238)) ([e705d44](https://github.com/planetaryescape/mxr/commit/e705d444139658946524ab74551a32c75808580d))

## [0.6.34](https://github.com/planetaryescape/mxr/compare/v0.6.33...v0.6.34) (2026-09-27)


### Features

* type times in words anywhere and see exactly when they resolve ([#236](https://github.com/planetaryescape/mxr/issues/236)) ([dfff3f4](https://github.com/planetaryescape/mxr/commit/dfff3f4e6d10e541a25709eed860db06f31012f7))

## [0.6.33](https://github.com/planetaryescape/mxr/compare/v0.6.32...v0.6.33) (2026-09-27)


### Features

* rebuild the web app to TUI parity and write AI drafts in your own voice ([#233](https://github.com/planetaryescape/mxr/issues/233)) ([faa154d](https://github.com/planetaryescape/mxr/commit/faa154d7c2be588165e370c2cbbdbfe0b5d48131))

## [0.6.32](https://github.com/planetaryescape/mxr/compare/v0.6.31...v0.6.32) (2026-09-24)


### Documentation

* make AGENTS.md the single agent instruction file ([0e8380f](https://github.com/planetaryescape/mxr/commit/0e8380ffbabb3e3fd3fde92236ab21c542912710))
* make ship it run the full release-please chain ([#231](https://github.com/planetaryescape/mxr/issues/231)) ([9a8b595](https://github.com/planetaryescape/mxr/commit/9a8b595d03c0bcddc4391af5246f1d54483ffe84))
* **marketing:** add agent meetup deck, notes and PDFs ([102f909](https://github.com/planetaryescape/mxr/commit/102f909d9e89a22b58d017d9536e1f69c34d87ea))
* **marketing:** add launch thread copy, cards and demo video ([159d47b](https://github.com/planetaryescape/mxr/commit/159d47b8af08e133c46a5db5de960d6a871b1ffa))
* **marketing:** add video library clips, posters and post copy ([dfb23d1](https://github.com/planetaryescape/mxr/commit/dfb23d10138b1cfc24f8ea7450d3426e5e4da37a))
* **plans:** add improvement plans 001-025 ([56dc79e](https://github.com/planetaryescape/mxr/commit/56dc79e8b4acd24b6266dc5359e6647c313f30cd))
* repoint stale CLAUDE.md citations in web crate comments ([aea133c](https://github.com/planetaryescape/mxr/commit/aea133cd61f3c5803213d18f9447bb5089ba33c9))

## [0.6.31](https://github.com/planetaryescape/mxr/compare/v0.6.30...v0.6.31) (2026-09-23)


### Bug Fixes

* stop background LLM work burning CPU on empty completions ([#229](https://github.com/planetaryescape/mxr/issues/229)) ([44568a7](https://github.com/planetaryescape/mxr/commit/44568a7909478d37d4fc2c1f28e59bb76b7e253e))


### Documentation

* add video gallery ([#222](https://github.com/planetaryescape/mxr/issues/222)) ([9160b1a](https://github.com/planetaryescape/mxr/commit/9160b1a12ef9dc5d3fb5513b45c68fe57183074f))
* define companion process boundary ([#228](https://github.com/planetaryescape/mxr/issues/228)) ([3319014](https://github.com/planetaryescape/mxr/commit/33190149e7ce6c06ea41356b8158e6fc7f5622d9))

## [0.6.30](https://github.com/planetaryescape/mxr/compare/v0.6.29...v0.6.30) (2026-08-26)


### Bug Fixes

* finish HCL Domino IMAP safeguards with a per-account connection cap, safe trace filtering, and working logging configuration ([#216](https://github.com/planetaryescape/mxr/issues/216)) ([90d86d1](https://github.com/planetaryescape/mxr/commit/90d86d10b9f9c8ecb07ee2526ca34071dd9ec91c))

## [0.6.29](https://github.com/planetaryescape/mxr/compare/v0.6.28...v0.6.29) (2026-08-25)


### Bug Fixes

* satisfy Rust 1.98 Clippy in semantic blob decoding and TUI mutation tests ([b05b46d](https://github.com/planetaryescape/mxr/commit/b05b46d4c102400a437c6d7d8db6d8e4731d4f40))

## [0.6.28](https://github.com/planetaryescape/mxr/compare/v0.6.27...v0.6.28) (2026-08-24)


### Bug Fixes

* remove a stale Linux dead-code expectation so full `-D warnings` Clippy runs stay green ([936f86a](https://github.com/planetaryescape/mxr/commit/936f86a4c7714c6455ae27cdee02197da4f5e756))

## [0.6.27](https://github.com/planetaryescape/mxr/compare/v0.6.26...v0.6.27) (2026-08-24)


### Bug Fixes

* isolate malformed IMAP FETCH responses so one bad Domino message no longer aborts initial sync ([#216](https://github.com/planetaryescape/mxr/issues/216)) ([e2b66e4](https://github.com/planetaryescape/mxr/commit/e2b66e48a2f1a35703b54ea026028a7ecedd932b))


### Documentation

* clarify that IMAP IDLE, Reply-To, and Gmail reply threading are shipped while `--format-version` remains deferred ([efa3a12](https://github.com/planetaryescape/mxr/commit/efa3a129f2e17d15bf89e5ad546d2860663ff95a), [dc5b543](https://github.com/planetaryescape/mxr/commit/dc5b543229e59fa8b2c8a19f1dd99840d18c10c7))

## [0.6.26](https://github.com/planetaryescape/mxr/compare/v0.6.25...v0.6.26) (2026-08-20)


### Bug Fixes

* feed every demo wait iteration into the stall clock ([#211](https://github.com/planetaryescape/mxr/issues/211)) ([0aa5aa1](https://github.com/planetaryescape/mxr/commit/0aa5aa1fa4e3524f15f065ea9c68d2d8bc99e1e3))
* keep a semantic index that never rebuilt from reading as fresh ([#210](https://github.com/planetaryescape/mxr/issues/210)) ([b0a1063](https://github.com/planetaryescape/mxr/commit/b0a1063aa1e4064c21659a35aebe68418a628b9b))


### Documentation

* close the post-[#179](https://github.com/planetaryescape/mxr/issues/179) drift (plan record, whole-address examples, background sync docs) ([#207](https://github.com/planetaryescape/mxr/issues/207)) ([c844e88](https://github.com/planetaryescape/mxr/commit/c844e8876c8ad442cb8d463afbaf91e1c031fb8c))
* record the Codex retro findings and triage verdicts ([#209](https://github.com/planetaryescape/mxr/issues/209)) ([ad72d3d](https://github.com/planetaryescape/mxr/commit/ad72d3d72f701564cecb47996bb1f7b102004785))
* scope the ten-minute stall claim to the waits that have one ([#208](https://github.com/planetaryescape/mxr/issues/208)) ([141ebbb](https://github.com/planetaryescape/mxr/commit/141ebbbb0924bb1e129b42b27f39c370d8ee0dac))

## [0.6.25](https://github.com/planetaryescape/mxr/compare/v0.6.24...v0.6.25) (2026-08-20)


### Bug Fixes

* flush async draft writes, deflake two test suites, clear npm audit debt, fix demo search examples ([#201](https://github.com/planetaryescape/mxr/issues/201)) ([7d0191a](https://github.com/planetaryescape/mxr/commit/7d0191a7bb532de40413d0602571b1add243e381))
* identify a daemon before adopting or killing it, and report status honestly ([#203](https://github.com/planetaryescape/mxr/issues/203)) ([b7ecd5a](https://github.com/planetaryescape/mxr/commit/b7ecd5a19b275e6629aa8fac320053b833b46623))
* keep dependent rows on the message id the store actually wrote ([#202](https://github.com/planetaryescape/mxr/issues/202)) ([a9d1a12](https://github.com/planetaryescape/mxr/commit/a9d1a12710f6cc4407e59624751017fa6393c9f4))
* keep semantic search and status honest about index passes ([#200](https://github.com/planetaryescape/mxr/issues/200)) ([7c43fe8](https://github.com/planetaryescape/mxr/commit/7c43fe88e1364720a973cdd9b736316cc9ff3e60))
* reject a contradicting daemon record, recheck identity before signalling ([#206](https://github.com/planetaryescape/mxr/issues/206)) ([b64258d](https://github.com/planetaryescape/mxr/commit/b64258dfc81522b5c5d2015a684fd424823a1b5a))

## [0.6.24](https://github.com/planetaryescape/mxr/compare/v0.6.23...v0.6.24) (2026-08-19)


### Features

* run sync in the background with live progress ([#197](https://github.com/planetaryescape/mxr/issues/197)) ([5d47cba](https://github.com/planetaryescape/mxr/commit/5d47cba2130ed5720e70b51909ced28aaf09f421))


### Documentation

* record the [#179](https://github.com/planetaryescape/mxr/issues/179) demo-timeout fix plan, results and follow-ups ([#198](https://github.com/planetaryescape/mxr/issues/198)) ([9155662](https://github.com/planetaryescape/mxr/commit/91556621529b11f43835652642614e37cab6d407))

## [0.6.23](https://github.com/planetaryescape/mxr/compare/v0.6.22...v0.6.23) (2026-08-19)


### Bug Fixes

* make the demo seed fast and memory-bounded ([#193](https://github.com/planetaryescape/mxr/issues/193)) ([f3e9402](https://github.com/planetaryescape/mxr/commit/f3e940221de370e14d3dd1b3deaff551f1859afc))

## [0.6.22](https://github.com/planetaryescape/mxr/compare/v0.6.21...v0.6.22) (2026-08-19)


### Bug Fixes

* stop mxr demo and mxr sync dying on the 120s IPC timeout ([#191](https://github.com/planetaryescape/mxr/issues/191)) ([f4e53e2](https://github.com/planetaryescape/mxr/commit/f4e53e22f4e95e1fe5042e4a0fe508a6e2566d6b))

## [0.6.21](https://github.com/planetaryescape/mxr/compare/v0.6.20...v0.6.21) (2026-08-19)


### Bug Fixes

* make semantic reindex fast, idempotent and observable ([#189](https://github.com/planetaryescape/mxr/issues/189)) ([d051d6c](https://github.com/planetaryescape/mxr/commit/d051d6c5cc062ba0ea786ea048663153be4c66c7))

## [0.6.20](https://github.com/planetaryescape/mxr/compare/v0.6.19...v0.6.20) (2026-08-19)


### Features

* add linked draft MCP workflow ([#185](https://github.com/planetaryescape/mxr/issues/185)) ([e7e975d](https://github.com/planetaryescape/mxr/commit/e7e975df0a65d3ac409991434f7225a0c8de21c8))


### Bug Fixes

* thread Gmail reply drafts on the parent conversation ([#186](https://github.com/planetaryescape/mxr/issues/186)) ([026c846](https://github.com/planetaryescape/mxr/commit/026c8463ae1419aa6b852b03c524a8e1f0627bad))

## [0.6.19](https://github.com/planetaryescape/mxr/compare/v0.6.18...v0.6.19) (2026-08-12)


### Bug Fixes

* keep release lockfile versions in sync ([7a7c9be](https://github.com/planetaryescape/mxr/commit/7a7c9be46f55652b530f05300ccef355fffc9c54))
* sync release workspace lock versions ([268e6eb](https://github.com/planetaryescape/mxr/commit/268e6eb46a8768c57a0dc5da8e3e2ff857be6fd9))

## [0.6.18](https://github.com/planetaryescape/mxr/compare/v0.6.17...v0.6.18) (2026-08-12)


### Features

* sync linked Gmail drafts in place ([0cf56ab](https://github.com/planetaryescape/mxr/commit/0cf56ab9ccd4807d0c258cef285a9b9e644d92eb))


### Bug Fixes

* keep fake provider inside crate boundary ([900f3db](https://github.com/planetaryescape/mxr/commit/900f3db5cb218f6435088a8c5665238ad807fa5d))

## [0.6.17](https://github.com/planetaryescape/mxr/compare/v0.6.16...v0.6.17) (2026-08-05)


### Features

* add draft parity across clients ([2db493f](https://github.com/planetaryescape/mxr/commit/2db493ff4ed0e32e04f87741303f0a283aa1a98e))


### Bug Fixes

* keep web lint gate green ([caaebf3](https://github.com/planetaryescape/mxr/commit/caaebf367d34fd75477944743e776da18341deb5))
* release web draft lock before async cleanup ([0ef2abb](https://github.com/planetaryescape/mxr/commit/0ef2abb6b3c99cfe9eb6e32588917ec59a979502))
* satisfy TUI draft clippy checks ([8fe8747](https://github.com/planetaryescape/mxr/commit/8fe8747bec9a3e80423e6a988a062bf737e81c8c))

## [0.6.16](https://github.com/planetaryescape/mxr/compare/v0.6.15...v0.6.16) (2026-08-04)


### Features

* schedule mail merge delivery ([555299c](https://github.com/planetaryescape/mxr/commit/555299c60ad6b5d4e8078008f4257867abec26a4))


### Bug Fixes

* install the mxr-mailmerge companion from the release archive ([c1ed03b](https://github.com/planetaryescape/mxr/commit/c1ed03b380f299633cac217fb80c13930699c25a))
* install the mxr-mailmerge companion from the release archive ([#175](https://github.com/planetaryescape/mxr/issues/175)) ([f1e0d2b](https://github.com/planetaryescape/mxr/commit/f1e0d2be1c6cd732dbf68163c0b97e20ab23ff2f))
* update postcss security patch ([5c8ced4](https://github.com/planetaryescape/mxr/commit/5c8ced4631cca695b5ac8e0f6429a170934a5624))


### Documentation

* document scheduled mail merge delivery ([06ed132](https://github.com/planetaryescape/mxr/commit/06ed132e194c762cfb8bf92c202bd4cde6ff70ea))
* sharpen landing page call to action ([d15a412](https://github.com/planetaryescape/mxr/commit/d15a412a68c1d01141040f41bd051b913728fd99))

## [0.6.15](https://github.com/planetaryescape/mxr/compare/v0.6.14...v0.6.15) (2026-07-31)


### Features

* native HTML email composition and a mail merge companion ([#167](https://github.com/planetaryescape/mxr/issues/167)) ([e3999de](https://github.com/planetaryescape/mxr/commit/e3999de199133add9a4b40d9fea91b011fdfc9be))
* redesign the mxr landing page ([ba7caf8](https://github.com/planetaryescape/mxr/commit/ba7caf8e3a4099829715ddc64fde885f74831b05))
* warm search before first use ([fef5d98](https://github.com/planetaryescape/mxr/commit/fef5d986013626732ff0043e02059fac3c2a3e93))


### Bug Fixes

* correct mxr docs and trust links ([1424ce1](https://github.com/planetaryescape/mxr/commit/1424ce14f69bf8dcc9f425c5d18ad894abda6aa3))
* correct mxr homepage commands and claims ([d642926](https://github.com/planetaryescape/mxr/commit/d6429260d25ab7b9e81fd7a99e3f32e90983c7a0))
* correct mxr site offline, mutation JSON, and agent examples ([c032e44](https://github.com/planetaryescape/mxr/commit/c032e44efd1645e27ef083472f398f8fdcb059e0))
* correct mxr site provider, agent, and aggregation copy ([1119496](https://github.com/planetaryescape/mxr/commit/111949688534e195caf1745c392f5d0ce25f2ad1))
* correct mxr site trust claims and reframe the API page ([78e2421](https://github.com/planetaryescape/mxr/commit/78e2421f9095b10dd0614241315fee02ef64998f))
* correct privacy credential storage and self-host site fonts ([1bdfae0](https://github.com/planetaryescape/mxr/commit/1bdfae013c91ac2563828ce6a8f52fff1f72b98e))
* disclose default network behaviour and bundle the API explorer ([a12ca66](https://github.com/planetaryescape/mxr/commit/a12ca6655f7913822a30997c8fe27a4b5df9067e))
* finish API inventory review fixes ([d5dc865](https://github.com/planetaryescape/mxr/commit/d5dc86563116c44bbf03c9128aa4357e89d392ab))
* improve mxr site accessibility and media ([ed1979d](https://github.com/planetaryescape/mxr/commit/ed1979d519379ec43af2a3b9b481a62a73ba49cd))
* make legal and code layouts accessible ([b9a6f0d](https://github.com/planetaryescape/mxr/commit/b9a6f0d674bdba0e379f631ccbedd7b7dcac170c))
* make mxr site tables, demo control, and video descriptions accessible ([a2ba4a2](https://github.com/planetaryescape/mxr/commit/a2ba4a2c1dada644846eaae8b58e5d1302e80986))
* qualify absolute automation claims in mxr site docs ([902e31e](https://github.com/planetaryescape/mxr/commit/902e31eea6a969f172612e3fe2af80dd49380643))
* reveal legal skip link on focus ([3a18ffb](https://github.com/planetaryescape/mxr/commit/3a18ffbccc58ca65ff52b281537adb6200b2198e))
* satisfy clippy in demo reconnect ([d94dc8d](https://github.com/planetaryescape/mxr/commit/d94dc8d2bfd875301c5de99e1ecdb4bd65c7314a))
* settle the hero demo's stale play refusals and idle posters ([45e2152](https://github.com/planetaryescape/mxr/commit/45e21523be25e0eede231abbbea8a3517832efb4))
* **site:** repair api explorer accessibility and broken doc links ([3793deb](https://github.com/planetaryescape/mxr/commit/3793deb5168cc22b155ef1cfdd6a86f3f194688b))
* state default-on network behaviour and all contacted services ([9687f50](https://github.com/planetaryescape/mxr/commit/9687f502f3547382eef72eea8486f74c3d1e3c12))
* state the real tracker-pixel behaviour and attachment dir overrides ([c0973e3](https://github.com/planetaryescape/mxr/commit/c0973e37c7249b53bbaf7f26f2d8ce972fe1e28a))


### Documentation

* sharpen mxr launch story ([c4ada04](https://github.com/planetaryescape/mxr/commit/c4ada04f5d9ca2d9f0e0c128ba575ab9c152e7bd))

## [0.6.14](https://github.com/planetaryescape/mxr/compare/v0.6.13...v0.6.14) (2026-07-29)


### Features

* **compose:** create drafts for any mode with --draft ([#163](https://github.com/planetaryescape/mxr/issues/163)) ([a98f064](https://github.com/planetaryescape/mxr/commit/a98f064ade420bd18fb8425fa3568d549b3e7c52))

## [0.6.13](https://github.com/planetaryescape/mxr/compare/v0.6.12...v0.6.13) (2026-07-22)


### Features

* **web:** send-as alias picker in compose From control ([#160](https://github.com/planetaryescape/mxr/issues/160)) ([c72bcdd](https://github.com/planetaryescape/mxr/commit/c72bcdd7e4902e18678917ac1e228c2eec8afd1b))

## [0.6.12](https://github.com/planetaryescape/mxr/compare/v0.6.11...v0.6.12) (2026-07-22)


### Bug Fixes

* let account repair run without daemon ([#158](https://github.com/planetaryescape/mxr/issues/158)) ([443fefb](https://github.com/planetaryescape/mxr/commit/443fefb7cff0edc339313299518b73a73a06747e))

## [0.6.11](https://github.com/planetaryescape/mxr/compare/v0.6.10...v0.6.11) (2026-07-22)


### Features

* **config:** disk-first credential storage ([#156](https://github.com/planetaryescape/mxr/issues/156)) ([f7d1865](https://github.com/planetaryescape/mxr/commit/f7d18657b2565c53969f65a7f375eb84a7626b32))

## [0.6.10](https://github.com/planetaryescape/mxr/compare/v0.6.9...v0.6.10) (2026-07-19)


### Features

* **cli:** add mxr daemon dial-stdio ([#146](https://github.com/planetaryescape/mxr/issues/146)) ([8f23b21](https://github.com/planetaryescape/mxr/commit/8f23b21cee409b838b6ebea792d6327dd12a59af))
* **compose:** per-message From-address selection (send-as aliases) ([#151](https://github.com/planetaryescape/mxr/issues/151)) ([c481a15](https://github.com/planetaryescape/mxr/commit/c481a153b87cd505f0c4ac7d341347e125f553fe))
* **llm:** harden mail-consuming prompts against injection ([#148](https://github.com/planetaryescape/mxr/issues/148)) ([569fbaa](https://github.com/planetaryescape/mxr/commit/569fbaa28dbdd899f71e4178131850273c7822f3))
* **transport:** community adapter kit — conformance suite, skeleton, blueprint ([#153](https://github.com/planetaryescape/mxr/issues/153)) ([17a4cf8](https://github.com/planetaryescape/mxr/commit/17a4cf88aeb92014f947ba9fc3b0516f37cd73ff))
* **transport:** TCP+token and stdio server transports, cmd:// connector ([#150](https://github.com/planetaryescape/mxr/issues/150)) ([3c158c7](https://github.com/planetaryescape/mxr/commit/3c158c7e4a3f787adacb64d44b1dbff225484d90))
* **transport:** transport traits with UDS as first adapter ([#149](https://github.com/planetaryescape/mxr/issues/149)) ([22d4cb3](https://github.com/planetaryescape/mxr/commit/22d4cb3551bb76197541509b3d1ce1faed55d6a8))


### Bug Fixes

* **ci:** resolve fmt, rustdoc, and architecture-boundary failures on main ([#154](https://github.com/planetaryescape/mxr/issues/154)) ([c4bd70f](https://github.com/planetaryescape/mxr/commit/c4bd70fc6adb1cc63d89028daed4f2aa3f6f66d8))
* **web:** attribute bridge IPC requests as web client ([#138](https://github.com/planetaryescape/mxr/issues/138)) ([3153127](https://github.com/planetaryescape/mxr/commit/31531275a41bf659ab12dff3634c4e7a91b94eaa))


### Refactoring

* **client:** introduce shared mxr-client IPC connection crate ([#143](https://github.com/planetaryescape/mxr/issues/143)) ([a2928e1](https://github.com/planetaryescape/mxr/commit/a2928e158e55492cfb8972a9ff371459d8473c00))
* **daemon:** extract generic serve core over AsyncRead+AsyncWrite ([#145](https://github.com/planetaryescape/mxr/issues/145)) ([11be5d3](https://github.com/planetaryescape/mxr/commit/11be5d36f60408aad4acc4d7e38478863ae6ac5c))


### Documentation

* `ssh -T localhost` in the phase-5 verification line. ([8f23b21](https://github.com/planetaryescape/mxr/commit/8f23b21cee409b838b6ebea792d6327dd12a59af))
* add transport adapter discovery and phased implementation plan ([#139](https://github.com/planetaryescape/mxr/issues/139)) ([d33c437](https://github.com/planetaryescape/mxr/commit/d33c4376364f512df2df62774dba4e1be7c0f6f4))
* email content is data, never instructions ([#141](https://github.com/planetaryescape/mxr/issues/141)) ([c0e8c2e](https://github.com/planetaryescape/mxr/commit/c0e8c2e81fea74719a2cd3b76b23e600f48500f4))
* retrieved mail is data, never instructions (AI layer principle 8) ([#142](https://github.com/planetaryescape/mxr/issues/142)) ([59f972c](https://github.com/planetaryescape/mxr/commit/59f972c2cfa887d56107b979c158ad2d1b2b5275))

## [0.6.9](https://github.com/planetaryescape/mxr/compare/v0.6.8...v0.6.9) (2026-07-16)


### Bug Fixes

* **tui:** make j/k scroll the thread reader and clamp overscroll ([#135](https://github.com/planetaryescape/mxr/issues/135)) ([3950d80](https://github.com/planetaryescape/mxr/commit/3950d80c09a1cb499cb61d97f02daa12353f2c81))


### Documentation

* update adapter skeleton and provider blueprint to post-refactor shapes ([#134](https://github.com/planetaryescape/mxr/issues/134)) ([2026ffc](https://github.com/planetaryescape/mxr/commit/2026ffcc1dd67ad7a43e7e0ac02d1065fd492f72))

## [0.6.8](https://github.com/planetaryescape/mxr/compare/v0.6.7...v0.6.8) (2026-07-16)


### Features

* **compose:** add --attach to reply, reply-all, and forward ([#131](https://github.com/planetaryescape/mxr/issues/131)) ([f4ff9f3](https://github.com/planetaryescape/mxr/commit/f4ff9f3c66b83804c705422a8283e095d5c96428))

## [0.6.7](https://github.com/planetaryescape/mxr/compare/v0.6.6...v0.6.7) (2026-07-14)


### Bug Fixes

* **sync:** bound provider network ops; show freshness in TUI sync status ([#128](https://github.com/planetaryescape/mxr/issues/128)) ([4e1e919](https://github.com/planetaryescape/mxr/commit/4e1e9197bd3922696d724f615bc03a4d144bcf6d))

## [0.6.6](https://github.com/planetaryescape/mxr/compare/v0.6.5...v0.6.6) (2026-07-11)


### Features

* edit stored drafts in place across CLI, TUI, and web ([#110](https://github.com/planetaryescape/mxr/issues/110)) ([6101d95](https://github.com/planetaryescape/mxr/commit/6101d950d85bab29eb5c375a6979d0b565bb491d))
* **imap:** file sent mail into the server Sent folder via APPEND (multi-account parity with Gmail) ([#127](https://github.com/planetaryescape/mxr/issues/127)) ([1ccc379](https://github.com/planetaryescape/mxr/commit/1ccc3794ba8920139fbda97cb89ec152295e0303))


### Bug Fixes

* **cli:** confine attachment downloads to --dir and sanitize mail filename ([#113](https://github.com/planetaryescape/mxr/issues/113)) ([b37ed29](https://github.com/planetaryescape/mxr/commit/b37ed2964c1edec0be27af4f87cc97fab5aa9964))
* **gmail:** decode message bodies by declared charset instead of dropping non-UTF-8 (P1 data loss) ([#114](https://github.com/planetaryescape/mxr/issues/114)) ([b60033c](https://github.com/planetaryescape/mxr/commit/b60033ce06901c7d8c4f0a4cbc4675d433549dcd))
* **imap:** harden sync, dates, labels, snippets, and command safety (13 fixes) ([#125](https://github.com/planetaryescape/mxr/issues/125)) ([467ca99](https://github.com/planetaryescape/mxr/commit/467ca9990c63d4c7b7acc8aa54c8ce854b3a5d27))
* make reader quote and activity/history truncation char-boundary safe ([#115](https://github.com/planetaryescape/mxr/issues/115)) ([c6e96f0](https://github.com/planetaryescape/mxr/commit/c6e96f0ffa35e59020265cd866a14a4a4c9542d3))
* **smtp:** guard cleartext auth, validate config, classify transient send errors ([#123](https://github.com/planetaryescape/mxr/issues/123)) ([f0777f6](https://github.com/planetaryescape/mxr/commit/f0777f6f8832b4d5f499eb78fbe89ffd7dcbd802))
* **store:** index messages(account_id, message_id_header) to kill dedup full-scans ([#109](https://github.com/planetaryescape/mxr/issues/109)) ([ca2b24b](https://github.com/planetaryescape/mxr/commit/ca2b24b8d12a382e08d575fe6b5719bdc04dba54)), closes [#107](https://github.com/planetaryescape/mxr/issues/107)
* **sync:** scope message-direction classification to the receiving account ([#126](https://github.com/planetaryescape/mxr/issues/126)) ([4bb1e7e](https://github.com/planetaryescape/mxr/commit/4bb1e7e78535060e0d4c8c43888cf3b608b8684d))
* **tui:** expand visual-line selection to thread rows, not flat envelope indices ([#112](https://github.com/planetaryescape/mxr/issues/112)) ([dabe2a8](https://github.com/planetaryescape/mxr/commit/dabe2a81a46506fc947ed02a232180bceafce2a7))
* unblock CI (crossbeam-epoch RUSTSEC-2026-0204 + clippy question_mark) ([#111](https://github.com/planetaryescape/mxr/issues/111)) ([c9b2f59](https://github.com/planetaryescape/mxr/commit/c9b2f59059fb490deefabe80dd2fa672cea3c70a))

## [0.6.5](https://github.com/planetaryescape/mxr/compare/v0.6.4...v0.6.5) (2026-07-05)


### Bug Fixes

* harden TUI event delivery, rendering, and draft hygiene ([#105](https://github.com/planetaryescape/mxr/issues/105)) ([2b1a93c](https://github.com/planetaryescape/mxr/commit/2b1a93cc8820de80940086bfd2f78e44930a62b6))

## [0.6.4](https://github.com/planetaryescape/mxr/compare/v0.6.3...v0.6.4) (2026-07-03)


### Documentation

* close out the issue backlog — crate docs, protocol docs, verified jq examples ([#101](https://github.com/planetaryescape/mxr/issues/101)) ([9ecaefa](https://github.com/planetaryescape/mxr/commit/9ecaefaaee73c579e6839b454eda0fabae545d9d)), closes [#9](https://github.com/planetaryescape/mxr/issues/9)

## [0.6.3](https://github.com/planetaryescape/mxr/compare/v0.6.2...v0.6.3) (2026-07-01)


### Documentation

* **site:** lead the hero with a terminal demo and a smaller headline ([#97](https://github.com/planetaryescape/mxr/issues/97)) ([321c72f](https://github.com/planetaryescape/mxr/commit/321c72f5e6d3d08a52abc2bbad19db2423e62783))

## [0.6.2](https://github.com/planetaryescape/mxr/compare/v0.6.1...v0.6.2) (2026-06-25)


### Bug Fixes

* align imap push capability reporting ([4608e61](https://github.com/planetaryescape/mxr/commit/4608e6169f477431426d991a936f7e6f1f840fea))
* harden attachment cache filenames ([c078037](https://github.com/planetaryescape/mxr/commit/c0780377b4142fddddd8fd91b50aad028c8d9e6c))
* limit bridge query token auth ([404d48c](https://github.com/planetaryescape/mxr/commit/404d48ca9be253bde00d2e84a5da1c9639df51c3))
* prevent sync loop deadlock when a detached sync wedges ([a57a53f](https://github.com/planetaryescape/mxr/commit/a57a53f5dae90c942717131c00e2d63576544654))
* redact inline secrets in bug reports ([60f1bad](https://github.com/planetaryescape/mxr/commit/60f1bad00111da79bc0638631429d33d09bfc133))
* serialize snooze provider mutations ([1a27e17](https://github.com/planetaryescape/mxr/commit/1a27e1757fccb5e8194512027e5b9fcd2a62ffda))
* validate gmail token refs ([e1daaaa](https://github.com/planetaryescape/mxr/commit/e1daaaa301bbebb54f4e363f164c63a7f3088be6))


### Refactoring

* remove dead daemon export module ([a3eca9f](https://github.com/planetaryescape/mxr/commit/a3eca9f7ef2442ded052fad3931e50d6ce221726))
* remove dead web state module ([cdac1a3](https://github.com/planetaryescape/mxr/commit/cdac1a3332cfff9406a9be08a40a97df3ed697f3))


### Documentation

* clarify deferred provider and bridge work ([b7ccdb1](https://github.com/planetaryescape/mxr/commit/b7ccdb143a1f5580e0126960c13e29df716a0d07))
* clarify reserved semantic config fields ([344edd7](https://github.com/planetaryescape/mxr/commit/344edd74903ea3f6ca0708e1025ef0cc37064471))
* clarify roadmap status ([5d10fed](https://github.com/planetaryescape/mxr/commit/5d10fed503c706ad6fc7199af494a01015611f29))
* clarify subscription replied counts ([4cf3e0e](https://github.com/planetaryescape/mxr/commit/4cf3e0e0e6d95d3a6e6b909e6c2f6665b4c9231f))
* reconcile triage gap tracker ([aedbcf2](https://github.com/planetaryescape/mxr/commit/aedbcf2241e7f32a203e8eb27b3fa0c4b67b4dee))
* remove stale web action comments ([359a659](https://github.com/planetaryescape/mxr/commit/359a659fabc2ecf0c429df117cc3e5cbc12326f1))
* **site:** surface examples on the landing and point the site at mxr.sh ([#93](https://github.com/planetaryescape/mxr/issues/93)) ([477ad84](https://github.com/planetaryescape/mxr/commit/477ad8465a2e9039b001e0d50003f3d945f63a00))
* triage root todo backlog ([4f048f9](https://github.com/planetaryescape/mxr/commit/4f048f9f1ba8405254ff61a100910787efc096d2))
* verify all install channels in release flow ([7f24d99](https://github.com/planetaryescape/mxr/commit/7f24d99c002cd3fe70032100f8a49e6bd23a88cb))

## [0.6.1](https://github.com/planetaryescape/mxr/compare/v0.6.0...v0.6.1) (2026-06-12)


### Features

* **tui:** context-aware shortcuts, help context filter, pending chord hint ([1b7f5b2](https://github.com/planetaryescape/mxr/commit/1b7f5b2359f84dd14cd3aacbbfdc3a25bc652dce))
* **tui:** per-account sync state in the sidebar ([1823fa7](https://github.com/planetaryescape/mxr/commit/1823fa7cd920ee5063efd359154131c6260cb6ee))
* **tui:** Phase 4 production polish ([cb8bc6d](https://github.com/planetaryescape/mxr/commit/cb8bc6dbe61968211afade22c297759b291c35a7))
* **tui:** status-bar spinner for in-flight work and bulk n/m progress ([2666d37](https://github.com/planetaryescape/mxr/commit/2666d371b5ccfa53ffbff3268346708c98fa6508))
* **tui:** toast notification queue for operation outcomes ([98709e8](https://github.com/planetaryescape/mxr/commit/98709e862f2450a4c5b97904f9906b6720d7c8ae))
* **web:** attachment size warning and per-file upload progress ([56802f0](https://github.com/planetaryescape/mxr/commit/56802f06f8cb53ac751111d160336f423ba5a37a))
* **web:** cmd+shift+enter sends and archives the source conversation ([b7ca1ed](https://github.com/planetaryescape/mxr/commit/b7ca1edda0469c1a297108640da6f820598e175a))
* **web:** global z shortcut undoes the last mail mutation ([16d5b51](https://github.com/planetaryescape/mxr/commit/16d5b518daf10dbb4b8b2b542d9075ed8884b3b9))
* **web:** inline/popout compose surfaces and safety-gated send ([c478e3b](https://github.com/planetaryescape/mxr/commit/c478e3b106bbb0184dc160c23ab72a923734b68e))
* **web:** maybe-include collaborator chips above recipients ([6f13fd1](https://github.com/planetaryescape/mxr/commit/6f13fd1238b24451505f19c29d2d3c54a3fd04b9))
* **web:** minimal formatting toolbar for the rich text editor ([14ba86e](https://github.com/planetaryescape/mxr/commit/14ba86e8411fa56bbeddc1e716e7efa29cbf780a))
* **web:** paginate subscription drilldown lens ([960039d](https://github.com/planetaryescape/mxr/commit/960039d99254c2ef996d05f147fa86c8b618d503))
* **web:** Phase 1 — keyboard foundation ([8181fa2](https://github.com/planetaryescape/mxr/commit/8181fa2161f1e195c059607fe07d8ae3b074673d))
* **web:** Phase 2 — compose redesign ([7bb5abb](https://github.com/planetaryescape/mxr/commit/7bb5abbf6e43029fc2f8ba1a48ca3bca62a02afc))
* **web:** Phase 3 parity backfill ([f3d7d18](https://github.com/planetaryescape/mxr/commit/f3d7d1897cf24a483f5cc89c3f058785d0e39204))
* **web:** recipient field RFC 5322 parsing and combobox a11y ([0c5870f](https://github.com/planetaryescape/mxr/commit/0c5870f1b6552b5e08f55fc4e43e1309bb873033))
* **web:** rename and delete labels from the label picker ([ae577dc](https://github.com/planetaryescape/mxr/commit/ae577dc89777bb619cdb5fda5663281d68cdaf95))
* **web:** safety-check + collaborators bridge routes, undo-send pref, send-later parser ([ba2225a](https://github.com/planetaryescape/mxr/commit/ba2225acb0fe8050f90d152fe1dc28f9dddec3b5))
* **web:** scoped shortcut registry, [/] triage keys, * sequences, pending-prefix chip ([9dbcc02](https://github.com/planetaryescape/mxr/commit/9dbcc021ed8a8b2a009e417d1e0f4cfea9e41539))
* **web:** screener account switcher and decisions tab ([d10801c](https://github.com/planetaryescape/mxr/commit/d10801cf3645a51537ad675a074d78a40e1ce79e))
* **web:** send later from compose with natural-language scheduling ([f375943](https://github.com/planetaryescape/mxr/commit/f375943d87d0e05d83d17cde002bbdbcdda4fd6c))
* **web:** signature picker dialog on cmd+shift+g ([3c6f9f4](https://github.com/planetaryescape/mxr/commit/3c6f9f43c7a9f39e191b6fd6d834bb0139c79db4))
* **web:** snippet picker dialog on cmd+semicolon ([e4c1f36](https://github.com/planetaryescape/mxr/commit/e4c1f36ae9e96a51be3a1aa477e08f652ee643bc))
* **web:** terminate OAuth poll on terminal states, add cancel + error surfacing ([6b75e21](https://github.com/planetaryescape/mxr/commit/6b75e21002ea843eada5b556e321a00c3976a5ac))
* **web:** thread/recipient briefings, expert finder, commitment resolve, relationship view ([e153b2e](https://github.com/planetaryescape/mxr/commit/e153b2e24146fe58864c90214b61a6652e493ed2))
* **web:** vim mode line and ex commands in markdown compose editor ([254f960](https://github.com/planetaryescape/mxr/commit/254f9601dd4f90513f920901e8d447b1e9e2a7e6))


### Bug Fixes

* Phase 0 — keybinding collision, IPC error surfacing, compose data-loss guards, global z undo ([c28ab7e](https://github.com/planetaryescape/mxr/commit/c28ab7eef88a0e2eec9d4cb93c78cec18687920c))
* **tui:** bind activity log to g y and guard against keybinding collisions ([e3b420a](https://github.com/planetaryescape/mxr/commit/e3b420a8c20cd1c03f28a3f3ff2dcc7b57b8676d))
* **tui:** name the request in IPC error messages and surface thread-load failures ([12abbc3](https://github.com/planetaryescape/mxr/commit/12abbc355083c30bbe92bbfc7b0f8f0b8f14c52e))
* **tui:** rustfmt the Phase 4 changes and drop an unused test import ([2833fbd](https://github.com/planetaryescape/mxr/commit/2833fbdfe2d1a544b0a032373474c4e63d45d65a))
* **tui:** surface failed pre-send safety checks in the send modal ([44280fe](https://github.com/planetaryescape/mxr/commit/44280fe33c0b5dc56f1f12f6934181b94efcc14d))
* **web:** close compose data-loss holes ([6cd92d7](https://github.com/planetaryescape/mxr/commit/6cd92d741dc4dd20f065e87f6332c49e7f099c4c))
* **web:** point e2e harness at target-cli binary, regen OpenAPI types ([8e7635f](https://github.com/planetaryescape/mxr/commit/8e7635f448020ff8c45b5f1bcfb2f70be73b3b2f))
* **web:** rustfmt the Phase 3 bridge handlers ([ad9a7e5](https://github.com/planetaryescape/mxr/commit/ad9a7e582ff5202ed3a0eda405c8bc7a5eab8a03))


### Refactoring

* **tui:** hygiene — shared centered_rect, theme contrast tokens, small-terminal guard, modal cleanup on tab switch ([1dac48b](https://github.com/planetaryescape/mxr/commit/1dac48bdebd4698726eced5144c817fa0d36c982))
* **web:** extract useComposeSession and ComposeEditorPanel from ComposeRoute ([be5f9a6](https://github.com/planetaryescape/mxr/commit/be5f9a656b9c8a0ef3f382201f2cffd4aab3e48d))

## [0.6.0](https://github.com/planetaryescape/mxr/compare/v0.5.63...v0.6.0) (2026-06-12)


### ⚠ BREAKING CHANGES

* **deps:** dependency major migration + upgrade-resilience follow-ups ([#84](https://github.com/planetaryescape/mxr/issues/84))

### Chores

* **deps:** dependency major migration + upgrade-resilience follow-ups ([#84](https://github.com/planetaryescape/mxr/issues/84)) ([af007b8](https://github.com/planetaryescape/mxr/commit/af007b8bb0362d6a2da79e6c913907e47fab6221))

## [0.5.63](https://github.com/planetaryescape/mxr/compare/v0.5.62...v0.5.63) (2026-06-11)


### Bug Fixes

* race-free daemon upgrade handoff + UTF-8-safe prompt truncation ([#82](https://github.com/planetaryescape/mxr/issues/82)) ([5077dc8](https://github.com/planetaryescape/mxr/commit/5077dc866ce0a8ca34253781864c5dd9f4ba5863))

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
