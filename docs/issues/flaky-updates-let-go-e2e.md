# Flaky e2e: the let-go dialog does not open after a reload

Status: resolved. Found: 2026-11-22 on main's merge commit for #1013 (e0e3042b6).

`let go of all acts on exactly the previewed cut, and u puts it back`
(`apps/web/e2e/updates.spec.ts`) failed in CI runs 45020688626 and 45050622055.

## What failed

After `page.reload()`, the unread check can pass before the action catalog
boots. The catalog waits for first contentful paint and then an idle callback.
If the test presses `E` before the catalog binds its keys, the key does nothing
and `updates-let-go-dialog` never appears. The first attempt already changes
seen flags in the shared daemon, so the retry has no unread peek button.

## The test waits for both handlers and keys

`markCatalogLoaded()` sets `data-keys-ready="true"` on the document's `html`
element. After reload, the test waits for that attribute and the visible
`updates-let-go-all` button before pressing `E`. The button and the `letGoAll`
handler share the `digest.let_go_line` condition. Catalog boot timing stays unchanged.
