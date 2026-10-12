# S09 candidate acceptance at the merged source snapshot

PR436 merged as `8faa88f1c11ec0f66eb4f37c5212189b00f74aa7`; its tree `0ddafa717c07beb1c2068da41f2a09352ff19212` matches reviewed candidate `d593f1e11bb19c4786e8c80df4e871566e04f25a`. Candidate binary SHA-256 is `a1bbb15a494794d3ad4386335ef30fac653ad471c149c82ab0fad3ec4b9eb6c9`. This packet records candidate acceptance only. Release PR438 proposes v0.6.92, but release publication and installed-client acceptance are not established here.

The candidate CLI/MCP journey passed four case groups in 65 seconds, including held cached reads, explicit activation, preparation of a separate reviewed draft, mixed protocol-version rejection, and one accepted 9,309-byte attachment. The actual native TUI journey kept both original drafts held, preserved the same attachment bytes through restore and preparation, and recorded no provider acceptance, scheduled attempt or runnable job. Fourteen negative cases passed while leaving the bundle unchanged. Root closed the owned processes after each journey.

The browser receipt in this packet is an earlier candidate baseline from binary `b0b46206`, not final `d593f1e1` or published-release acceptance. Repeat the actual browser journey against the published artifact before closing S09.

CI run [38259491962](https://github.com/planetaryescape/mxr/actions/runs/38259491962) passed 4,291 fast tests with 9 skipped and 4,337 semantic tests with 10 skipped. The Web E2E job reported 384 passing tests, two flaky tests and four passing speed checks (23.2 seconds). OpenAPI run [38259491823](https://github.com/planetaryescape/mxr/actions/runs/38259491823) passed. No cause is assigned to retried web assertions; see the [follow-up](../../../../issues/s09-web-e2e-retry-flakiness.md).

These results use isolated profiles and synthetic mail. They do not establish acceptance of a release installer, Homebrew installation, live-provider credentials, remote HTTP access, a VPS, Tailscale, or an iOS client. The final installed packet should be stored separately so this candidate history remains pinned.
