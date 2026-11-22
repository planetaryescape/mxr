# The CLI dispatch boxes command futures to keep the debug poll frame small

**Status:** open · **Found:** delete-comment sweep, `b-daemon-rest` · **Severity:** nice-to-have

## What happens

`crates/daemon/src/lib.rs` builds each command's future through `boxed` (type `CommandFuture`)
and runs `Command::Completions` on `spawn_blocking`. Both exist so Clap's command tree and the
large `dispatch` future do not share one debug-build stack frame. Two comments said so. Nothing
in code or tests states or checks it.

## Smallest fix

Add a test that runs `dispatch` on a small-stack thread in a debug build, then delete the
workaround or keep it with that test as the explanation.
