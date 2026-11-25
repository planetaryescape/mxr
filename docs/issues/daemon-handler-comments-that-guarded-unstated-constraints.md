# Handler constraints that lived only in comments

Status: open. Found: delete-comment sweep of `crates/daemon/src/handler`.

These comments were deleted. Each named a constraint with no test or type behind it:

- `error.rs`, `HandlerError` `Display`: it must stringify to exactly what `.map_err(|e| e.to_string())` produced before the migration, or the IPC error wire changes. Add a test that pins the text of each variant.
- `mod.rs`, `handle_request_with_peer`: `peer` is carried "for phase 5's per-transport policy" and no policy reads it. Either use it or remove the parameter.
- `mutations.rs`, the `build_safety_context` integration tests: a doc comment said one test "FAILS today" because `contact_styles` was empty. The code now fills it. The test that pinned this, `contact_styles_loaded_emits_tone_warning`, was removed in the 2026-11-22 test cull, so the tone warning with loaded contact styles has no test. See [test-cull-unguarded-decisions.md](test-cull-unguarded-decisions.md).

Smallest fix: one pinned-text test for `HandlerError`, and decide the `peer` parameter.
