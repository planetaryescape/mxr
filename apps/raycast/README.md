# mxr for Raycast

Now, the five modes and the inbox as Raycast commands, talking to the local mxr daemon
over its HTTP bridge. User guide: `site/src/content/docs/guides/raycast.md`.

```bash
npm ci
npm run dev        # import into Raycast and rebuild on change
npm run typecheck  # tsc against the bridge's generated types
npm run lint       # ray lint
```

Types come from `apps/web/src/api/generated.ts` (type-only import), so the
extension tracks the same OpenAPI contract as the web app. Regenerate that
file with `npm run gen:types` in `apps/web` after changing bridge routes. The
mailbox, search and thread routes are not in the spec yet, so
`src/bridge/mail.ts` describes the fields it reads. The reader splits plain
text with the web app's `features/thread/textSegments.ts`, bundled in, so
quotes and signatures fold the same way in both.

Lint keeps "mxr" lowercase in titles, which the title-case rule flags as
warnings.
