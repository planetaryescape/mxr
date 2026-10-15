# TUI highlights mark the first match, not where they were made

Status: logged, not fixed. Found 2026-10-15 on main `c806fec34`.

## What

Since v0.6.103 (#473) a highlight can carry the paragraph it was made in, so the web reader marks that occurrence instead of the first one. The TUI sends `paragraph: None` (`crates/tui/src/app/reading_actions.rs:588`), so TUI highlights of repeated words still mark the first match.

## Follow-up

Send the paragraph index from the TUI. Check first that the TUI counts the daemon's paragraph list the same way the web plain reader does, and which `view` it saves, because `marksForView` only keeps an anchor in the view it was made in.
