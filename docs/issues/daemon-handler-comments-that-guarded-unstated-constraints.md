# Handler constraints that lived only in comments

Status: open. Found: delete-comment sweep of `crates/daemon/src/handler`.

These comments were deleted. Each named a constraint with no test or type behind it:

- `error.rs`, `HandlerError` `Display`: it must stringify to exactly what `.map_err(|e| e.to_string())` produced before the migration, or the IPC error wire changes. Add a test that pins the text of each variant.
- `mod.rs`, `handle_request_with_peer`: `peer` is carried "for phase 5's per-transport policy" and no policy reads it. Either use it or remove the parameter.
- `mutations.rs`, the `build_safety_context` integration tests: a doc comment said one test "FAILS today" because `contact_styles` was empty. The code now fills it (`contact_styles_loaded_emits_tone_warning` passes its field). The note was stale and is gone, so no action beyond confirming that test runs in CI.

Smallest fix: one pinned-text test for `HandlerError`, and decide the `peer` parameter.
