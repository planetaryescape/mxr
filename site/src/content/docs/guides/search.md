---
title: Search workflow
description: Use search as the primary navigation model in mxr.
---

## Core idea

mxr treats search as navigation, not a bolt-on filter. Search results drive the TUI mail list, saved searches, exports, and batch mutations.

## Common patterns

```bash
mxr search "from:alice unread"
mxr search "label:work has:attachment"
mxr search "subject:\"quarterly review\" after:2026-01-01"
mxr search "unsubscribe"
mxr search "label:inbox" --format ids
```

## Dedicated search page

The TUI has both:

- Inline search with `/`
- A dedicated Search page for a broader search-and-preview workflow

The dedicated page gives you:

- Query input
- Result list
- Preview pane
- Normal open flow into mailbox/thread interaction

## Useful combinations

- Use `mxr search ... --format ids | xargs ...` for shell pipelines.
- Save high-value searches in the TUI sidebar for recurring workflows.
- Use `mxr count QUERY` for quick status-bar or script integration.
- Use `mxr export --search QUERY --format mbox` to archive slices of mail.

## TUI flow

1. Press `/` to start a search.
2. Refine with fields such as `from:`, `to:`, `subject:`, `label:`, `before:`, `after:`.
3. Use `n` and `N` to move between matches.
4. Open results with `Enter` or `o`.
5. Use bulk select to mutate or export the result set.

## Saved searches

Saved searches are not a secondary convenience. They are persistent inbox lenses and appear in:

- The sidebar
- Command palette
- CLI via `mxr saved`

Common flow:

```bash
mxr saved add urgent "label:inbox unread from:boss@example.com"
mxr saved list
mxr saved run urgent
```
