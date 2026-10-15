# Paper trail has lost its shortcut and sidebar entry

Status: logged, needs a product decision. Found 2026-10-15 on main `c806fec34`.

## What

- In the TUI, `gp` now opens Updates (`crates/tui/src/keybindings.rs:525`), but the desk still advertises "Paper trail … this week (g p)" (`crates/tui/src/ui/desk_lens.rs:260`).
- The web app has no navigation action for Paper trail, only the `/paper-trail` route and the desk link.
- `site/src/content/docs/guides/reading-and-paper-trail.md:39` still says to pick Paper trail in the sidebar.

## Follow-up

Decide whether Paper trail stays a place you can navigate to. If it does, give it a key and a sidebar entry again. If not, fold it into Archive or Updates, drop the desk hint and update the guide.
