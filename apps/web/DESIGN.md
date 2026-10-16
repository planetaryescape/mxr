# Web app design

The web app's design source is [docs/blueprint/26-web-design-concept.md](../../docs/blueprint/26-web-design-concept.md) (the concept: one lit item, three ink depths, a hierarchy per screen) and [docs/blueprint/27-web-design-system.md](../../docs/blueprint/27-web-design-system.md) (the tokens, type roles, shape rule, page frame and the lints that keep them shared). Decision D137 in the [decision log](../../docs/blueprint/15-decision-log.md) records why this is separate from the site's root `DESIGN.md`.

The colour roles per theme are generated: [src/styles/themes.ts](src/styles/themes.ts) holds each theme's inputs, [src/styles/themeGenerator.ts](src/styles/themeGenerator.ts) the rules, and `npm run gen:themes` writes [src/styles/themes.generated.css](src/styles/themes.generated.css), which [src/styles/tokens.css](src/styles/tokens.css) imports. [src/styles/type.css](src/styles/type.css) has the type roles. Every page renders [src/components/PageFrame.tsx](src/components/PageFrame.tsx).

`npm run lint` holds the rules: `scripts/check-type-scale.mjs`, `check-motion.mjs`, `check-shape.mjs` and `check-frames.mjs`. `e2e/design-gate.spec.ts` screenshots the main pages in four themes at three widths.

## Adding a theme

A theme is three values and a short list of exceptions. Add an entry to `THEMES` in `src/styles/themes.ts`:

- `dark`: picks the polarity. Light themes step their lamp, rules and surfaces darker than the desk; dark themes step them lighter.
- `base`: the desk, any CSS colour. Room, lamp, overlay, rules and surfaces are lightness steps from it, so choose the desk first.
- `accent`: `--signal`, the lamp's colour. Not the CSS `--accent`, which is a neutral hover surface.
- `contrast`: the contrast ratios the three text tiers reach on the desk. Use `CONTRAST.dark`, `CONTRAST.light` or `CONTRAST.soft` (Eclipse's lower glare).

Then add the id to `ThemeId` in `themeGenerator.ts` and to the theme picker and the theme store, and run `npm run gen:themes`. `npm run lint` fails when the generated CSS is stale, and `themes.test.ts` fails when a theme's text tiers miss their targets, when text on a filled control is under 4.5:1, or when generated Midnight drifts more than ΔE OK 2 from the Midnight that shipped before the generator.

When the rules don't give a good result, use an option rather than editing generated output:

- `ink`: text hue and per-tier chroma, when the ink should differ from the desk's tint. Light uses it for navy ink on a barely tinted desk.
- `neutralMarks`: unread marks and count badges use `--text-1` instead of the accent. Paper uses it because rust reads as error red.
- `overrides`: a role name to a colour, replacing what the rules produce. Eclipse and Paper override `chart-2` (and Paper `chart-5`) because their accents sit next to the default violet and orange chart slots. Overrides to a role that doesn't exist throw.

Contrast-increased mode (the explicit preference and `prefers-contrast: more`) is generated too: `muted-foreground` and `subtle-foreground` are re-solved to 11.4 and 9.7 on dark themes and 8.5 on light ones, borders and input borders step further from the desk, the ring moves further from the desk, and the accent becomes `--primary` darker or lighter only if it is under 6.5:1 on the desk. The result is the `[data-contrast="increased"][data-theme="..."]` block per theme.

Colours that mark counts and unread items use `--unread-marker` and `--unread-marker-foreground` (`text-unread-marker`, `bg-unread-marker`), never `text-primary` or `text-signal`.
