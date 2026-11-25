# The OpenAPI job's path filter misses `crates/compose`

**Status:** open · **Found:** 2026-11-22, on test-cull PRs #1029 and #1031

The "Spec validates as OpenAPI 3.1" job (`.github/workflows/openapi-conformance.yml`) runs only when one of its listed paths changes. The list includes `crates/web/**`, `crates/protocol/**` and `crates/core/**`, but the generated spec also takes descriptions from doc comments in `crates/compose/src/frontmatter.rs`.

PR #1018 removed two of those doc comments. The job did not run, so `site/public/openapi.json` went stale on `main`. Two later PRs that did not touch the API failed with "OpenAPI artifact is stale". #1029 regenerated the spec.

To check whether the committed spec is current:

```bash
bash site/scripts/dump-openapi-spec.sh --check
```

## Fix

Add `crates/compose/**` to both `paths` lists in the workflow. Any other crate whose types reach the spec needs the same, so list the crates the spec's types come from before editing.
