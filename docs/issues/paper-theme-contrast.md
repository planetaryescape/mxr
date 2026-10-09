# Paper theme primary text misses minimum contrast

Status: logged, not fixed.

## What

On the Paper theme, two small primary-colored labels fall below the 4.5:1 text contrast minimum: the account monogram is 4.32:1 (`#bb401b` on `#f6e1da`), and the Now rail count is 4.29:1 (`#bb401b` on `#e9e4dd`). The closed-menu WCAG audit reported one serious `color-contrast` rule with these two nodes.

## Evidence

Observed at palette candidate `a5fbb014aef848a8542fa5f8a8e6bd38c690b89f` (tree `3cc81c281c07c18b95b6b2064990df12d475d976`), with the Theme menu closed. The raw audit is `/Users/bhekanik/code/planetaryescape/.orchestrate/ux-polish/pr321-evidence/paper-closed-wcag.json`; the full audit, screenshot, and other theme results are in `/Users/bhekanik/code/planetaryescape/.orchestrate/ux-polish/pr321-evidence/README.md`.

The relevant Paper tokens are `--primary`, `--primary-muted`, and `--sidebar-primary` in `apps/web/src/styles/tokens.css`. They are identical at the reviewed #319 base `2e50d87ccac91d3b867866ed7a1c34db4364f05e`; this palette candidate only adds `--fg-2` in the Paper block. The failures therefore predate this palette change.

## Follow-up

Adjust the Paper primary and muted-primary colors so these labels meet 4.5:1, then rerun the closed-menu WCAG audit and check the accent remains legible in the Paper theme.
