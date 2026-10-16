# Web app design

The web app's design source is [docs/blueprint/26-web-design-concept.md](../../docs/blueprint/26-web-design-concept.md) (the concept: one lit item, three ink depths, a hierarchy per screen) and [docs/blueprint/27-web-design-system.md](../../docs/blueprint/27-web-design-system.md) (the tokens, type roles, shape rule, page frame and the lints that keep them shared). Decision D137 in the [decision log](../../docs/blueprint/15-decision-log.md) records why this is separate from the site's root `DESIGN.md`.

The roles as CSS are in [src/styles/tokens.css](src/styles/tokens.css) (ink, text tiers, signal colours per theme) and [src/styles/type.css](src/styles/type.css) (the type roles). Every page renders [src/components/PageFrame.tsx](src/components/PageFrame.tsx).

`npm run lint` holds the rules: `scripts/check-type-scale.mjs`, `check-motion.mjs`, `check-shape.mjs` and `check-frames.mjs`. `e2e/design-gate.spec.ts` screenshots the main pages in four themes at three widths.
