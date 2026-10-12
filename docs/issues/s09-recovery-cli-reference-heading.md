# The generated recovery reference heading drops a hyphen

At release source `edf75f83368fc4afc3a9204e2d502773cc2950b2`, the generated page `site/src/content/docs/reference/cli/recovery.md` renders the subcommand heading as `mxr recovery prepare draft`, while the command synopsis on the same page correctly uses `mxr recovery prepare-draft`.

Reproduce from `site/` with `mise exec node@24.21.0 -- npm run generate`, then inspect the recovery page heading. The generator reads `crates/daemon/tests/snapshots/cli_help__cli_help_recovery_prepare_draft.snap`. In `site/scripts/generate-cli-reference.mjs`, `snapshotKeyToTokens` splits the snapshot key on `_` at line 421, the subcommand group joins tokens with spaces at line 643, and the page renderer emits that value in the heading at line 686. These references identify the observed transformation; this issue does not change the generator or manually edit generated output.

The command synopsis and direct backup guide remain correct. Preserve the hyphen in the rendered heading when updating the generator.
