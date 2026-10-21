# mxr for Raycast

Now and the five modes as Raycast commands, talking to the local mxr daemon
over its HTTP bridge. User guide: `site/src/content/docs/guides/raycast.md`.

```bash
npm ci
npm run dev        # import into Raycast and rebuild on change
npm run typecheck  # tsc against the bridge's generated types
npm run lint       # ray lint
```

Types come from `apps/web/src/api/generated.ts` (type-only import), so the
extension tracks the same OpenAPI contract as the web app. Regenerate that
file with `npm run gen:types` in `apps/web` after changing bridge routes.

Lint keeps "mxr" lowercase in titles, which the title-case rule flags as
warnings.
