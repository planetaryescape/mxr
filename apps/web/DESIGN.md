# Web app design

The web app's design source is [docs/blueprint/26-web-design-concept.md](../../docs/blueprint/26-web-design-concept.md) (the concept: one lit item, three ink depths, a hierarchy per screen) and [docs/blueprint/27-web-design-system.md](../../docs/blueprint/27-web-design-system.md) (the tokens, type roles, shape rule, page frame and the lints that keep them shared). Decision D137 in the [decision log](../../docs/blueprint/15-decision-log.md) records why this is separate from the site's root `DESIGN.md`.

The colour roles per theme are generated: [src/styles/themes.ts](src/styles/themes.ts) holds each theme's inputs, [src/styles/themeGenerator.ts](src/styles/themeGenerator.ts) the rules, and `npm run gen:themes` writes [src/styles/themes.generated.css](src/styles/themes.generated.css), which [src/styles/tokens.css](src/styles/tokens.css) imports. [src/styles/type.css](src/styles/type.css) has the type roles. Every page renders [src/components/PageFrame.tsx](src/components/PageFrame.tsx).

`npm run lint` holds the rules: `scripts/check-type-scale.mjs`, `check-motion.mjs`, `check-shape.mjs`, `check-frames.mjs` and `check-ai-marks.mjs`. `e2e/design-gate.spec.ts` screenshots the main pages in four themes at three widths.

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

## Motion

Motion explains change and never decorates (D134). Durations are the `--motion-duration-*` tokens; only transform and opacity move; `check-motion.mjs` fails anything else.

- **Leaving.** A done row folds away in 120ms (`row-fold` on `Row`'s `leaving`) while the lamp is already on the next row. `lib/leaving.ts` decides whether it folds: a pointer always does, a key only for the first press of a burst (presses within 600ms of the last one drop their row at once), and reduced motion never does. Messages folds through `useModeDone().folding`, To do and Updates through their `leaving` sets.
- **Keys never animate.** A component that takes a `via: "pointer" | "key"` prop (`LampVia`, `LeaveVia`) may start a transition or animation only behind `via === "pointer"` or `leaving`. `check-motion.mjs` enforces it.
- **Arriving.** A reader pane mounts inside `.reader-arrive`: 16px from the right with a fade over 180ms; reduced motion keeps the 120ms fade.
- **Overlays.** `scrimMotion` fades a scrim in over 120ms and `sheetMotion` rises a sheet 8px over 180ms. Dialogs, alert dialogs, the More sheet and the compose sheet use them.
- **Landing.** `Row` takes `land`, the message and thread ids it stands for; when the assistant or a sync changes one of them it wears a 2px signal underline that fades over 600ms (`landingStore`). The assistant's lists catch up after the mark, so a changed row is held for the 600ms; a sync's new row appears with the mark and nothing is held.
- **Finishing.** The low tide plays in `PageEmpty`'s `moment` slot, once per mode per day (`features/low-tide/lowTideMemory.ts`): Messages when Your turn is clear, To do when nothing is due today or earlier, Updates when its digest is let go. Archive has none. Reduced motion shows the words and the scene's still frame, with nothing animating (a picture is not motion; D134).
- **Toasts.** One at a time, 5 seconds (8 with Undo), a single line, in the corner that leaves the lit row clear (`toastPlacement.ts`). See `docs/web-app-controls.md`.
- **Skeletons.** `PageSkeleton` bars pulse only without reduced motion. A page that loads through `useDelayedPending` shows nothing for 150ms and holds a skeleton shown for at least 400ms.

## Icons for model features

No icon stands for AI (D141). A model feature is named for what it does and wears a task icon if the slot needs one: `PenLine` to draft, `TextQuote` to summarize. Not the sparkle, a wand or a robot, and not the accent colour. Who wrote the output is shown in words by `ModelMark` (D136). `check-ai-marks.mjs` fails the lint on those icons or a sparkle emoji.
