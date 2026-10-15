# The privacy page doesn't list Reading's network requests

Status: logged, not fixed (legal page, needs BK). Found 2026-10-15 on main `c806fec34`.

## What

`site/src/pages/privacy.md` lists every network request mxr makes, but it leaves out two:

- Fetching the article a Reading item links to, when you ask for it (`crates/daemon/src/handler/reading/article.rs`, `crates/reading/src/fetch.rs`).
- Marginalia: sending an item when you press `M`, and reading highlights back after syncs (`crates/reading/src/marginalia.rs`, `crates/daemon/src/handler/reading/marginalia.rs`). This goes to the Marginalia URL you connected.

## Follow-up

Add both to the list, with when each happens and what is sent.
