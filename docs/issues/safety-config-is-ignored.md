# The documented safety config is ignored

Status: logged, not fixed. Found 2026-10-15 on main `c806fec34` during a docs drift audit.

## What

`site/src/content/docs/reference/config.md` and `site/src/content/docs/guides/pre-send-safety.md` document `[safety.recipients]` and `[safety.tone]`, but setting them does nothing. `MxrConfig` has no `safety` field (`crates/config/src/types.rs` has only `safety_policy`), and the send path always builds the defaults: `crates/daemon/src/handler/mutations.rs:385` calls `mxr_safety::SafetyConfig::default()`.

The pre-send safety guide also gives `formality_delta_threshold = 0.25`; the code default is `0.4` (`crates/safety/src/config.rs:32`).

## Follow-up

Decide whether to wire `[safety]` into `MxrConfig` and pass it to the send path, or remove the section from both pages. Either way, correct the threshold default in the guide.
