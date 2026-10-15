# `mxr records` suggests a command that fails

Status: logged, not fixed. Found 2026-10-15 on main `c806fec34`.

## What

`mxr records` prints `Issuer page: mxr records --issuer "…"` (`crates/daemon/src/commands/records.rs:478`). `--issuer` exists only on `mxr records list` and `mxr records export`, so the suggested command errors. `site/src/content/docs/guides/archive.md` quotes this output, so the guide was left as is until the code changes.

## Follow-up

Print `mxr records list --issuer "…"`, then update the quoted output in the Archive guide.
