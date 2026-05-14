# Phase 6 — Web

Goal: web parity with the TUI activity screen. Bridge routes for every IPC verb; React page at `/activity` with DataTable + filter sidebar + detail drawer + bulk redact.

Follows the web-app conventions in [`docs/web-app/00-overview.md`](../web-app/00-overview.md). Reuse shadcn/ui patterns. No desktop imports; this is a from-scratch surface in `apps/web/`.

## Deliverables

### Bridge (Rust)

1. Routes added to `crates/web/src/routes_v6.rs`:
   - `GET /v6/activity`
   - `GET /v6/activity/count`
   - `GET /v6/activity/stats`
   - `GET /v6/activity/top`
   - `POST /v6/activity/export`
   - `POST /v6/activity/redact`
   - `POST /v6/activity/prune`
   - `POST /v6/activity/pause`
   - `POST /v6/activity/resume`
2. OpenAPI schema regenerated; types flow into `apps/web/src/api/generated.ts`.
3. PARITY_MATRIX.md entry for the activity surface.

### Frontend (React)

1. Route `apps/web/src/routes/activity.tsx`.
2. Route `apps/web/src/routes/activity.$id.tsx` (detail drawer).
3. Component tree under `apps/web/src/components/activity/` (see file layout below).
4. Filter sidebar with shadcn primitives.
5. Virtualized DataTable (TanStack Table + Virtual).
6. Bulk-select + bulk-redact flow with confirmation dialog.
7. Export modal (CSV / JSON / NDJSON + filter summary).
8. Pause toggle in top bar with state indicator.
9. Empty state + loading state + error state.
10. Playwright e2e smoke against a daemon backed by `provider-fake`.

## Bridge route shapes

Inputs use query strings for GET; JSON body for POST. Each route is a thin proxy to the IPC verb.

```
GET /v6/activity
  ?since=ISO_OR_DURATION
  &until=ISO_OR_DURATION
  &source=tui&source=cli                        # repeatable
  &action=mail.archive                          # repeatable
  &prefix=mail.
  &target_kind=thread
  &target_id=...
  &tier=important                               # repeatable
  &account=...
  &query=...                                    # FTS5
  &include_redacted=false
  &limit=50
  &cursor=1715592090123,4321
→ 200 { entries: [...], next_cursor: { ts, id } | null }
```

```
GET /v6/activity/stats?since=7d&until=now&group_by=action|day|source|target_kind|hour
→ 200 { buckets: [{ key, count }] }
```

```
POST /v6/activity/export
{ "filter": {...}, "format": "csv" | "json" | "ndjson", "inline_only": false }
→ 200 (inline) { format, count, body: "..." }
or
→ 200 (over 1 MiB) { format, count, path: "/Users/.../activity-export-...csv" }
```

```
POST /v6/activity/redact
{ "ids": [1, 2], "filter": null, "dry_run": false }
→ 200 { count, dry_run }
```

Auth: same bridge token model as every other route. Loopback-only enforced.

## React file layout

```
apps/web/src/
  routes/
    activity.tsx                          # list page
    activity.$id.tsx                      # detail drawer route (modal)
  components/activity/
    ActivityTable.tsx                     # virtualized table
    ActivityRow.tsx                       # row component
    ActivityFilterSidebar.tsx             # date range, source, action, tier, query
    ActivityDetailDrawer.tsx              # right-side panel
    ActivityExportDialog.tsx              # format + summary + download
    ActivityRedactDialog.tsx              # confirm + dry-run preview
    ActivityPauseControl.tsx              # top-bar pause toggle
    ActivityEmptyState.tsx
    ActivityStatsSidebar.tsx              # right rail with quick summary (top 5 actions, total count)
  hooks/
    useActivityList.ts                    # TanStack Query, infinite query for pagination
    useActivityStats.ts
    useActivityPause.ts
  lib/
    activityFormatters.ts                 # context_json → human strings per action
```

## Page composition

```
+--------------------------------------------------------------+
| Topbar                                          [paused: no] |
+------+-------------------------------------------+-----------+
|      |                                           |           |
| Side | DataTable (reverse-chron, virtualized)    | Stats     |
| Filt |                                           | rail      |
| er   |                                           |           |
|      |                                           |           |
+------+-------------------------------------------+-----------+
| Status bar: 142 rows · cursor · bulk-select count            |
+--------------------------------------------------------------+
```

Bulk selection: checkboxes on each row. Bulk bar appears at top when any row is selected, with `Redact selected` and `Export selected` actions.

## DataTable columns

| Column | Width | Render |
|---|---|---|
| Checkbox | 40px | selection |
| TS | 180px | local time, with `relative` tooltip |
| Source | 80px | colored badge (`tui`/`cli`/`web`/`daemon`) |
| Action | 160px | monospace token |
| Target | 220px | linkified when applicable |
| Context | flex | per-action formatter (see below) |
| Tier | 60px | dot + tooltip |
| Redacted | 80px | strike + "redacted" label when true |

## Per-action context formatters

In `lib/activityFormatters.ts`. Examples:

```ts
const formatters: Record<string, (ctx: any, row: ActivityEntry) => ReactNode> = {
  "mail.archive": (ctx, row) =>
    ctx?.count > 1 ? `bulk: ${ctx.count} threads` : "1 thread",
  "search.run": (ctx) => (
    <span>
      “{ctx?.query}” <span className="muted">→ {ctx?.result_count} results</span>
    </span>
  ),
  "mail.snooze": (ctx) => `until ${formatRelative(ctx?.until)}`,
  "mail.send": (ctx) => <>to: <code>{ctx?.recipients?.to?.[0]?.email ?? "…"}</code></>,
  "draft.send": (ctx) => <>{ctx?.subject ? <em>{ctx.subject}</em> : "draft"}</>,
  "view.open_screen": (ctx) => ctx?.screen ?? "",
  "link.click": (ctx) => <a href={ctx?.url} target="_blank" rel="noreferrer noopener">{ctx?.url}</a>,
  // default
  __default: (ctx) => ctx ? <code className="text-xs">{JSON.stringify(ctx).slice(0, 120)}</code> : null,
};
```

Keep formatters thin and pure — no IPC calls. Use shadcn's `Tooltip` for full context on hover.

## Detail drawer

- Right-side `Sheet` (shadcn) at `?detail=<id>` route param.
- Fetches single row via `useQuery(["activity", id])`. If row already in list cache, reuse.
- Sections:
  - Header: action token + timestamp + source.
  - Target: clickable to jump to thread/draft/search.
  - Context: full JSON (collapsible tree).
  - Actions: `Redact`, `Open target`, `Export this row`.

## Filter sidebar

- Date range picker (presets: 1h, 24h, 7d, 30d, custom).
- Source: multi-checkbox.
- Action: combobox with autocomplete from server-provided action list (Phase 6 includes endpoint `GET /v6/activity/actions` that returns the catalog) — defer if scope-creep, hardcode the catalog client-side for v1.
- Action prefix: short text input.
- Target kind: select.
- Tier: tri-checkbox.
- Query: text input with FTS5 hint.
- Include redacted: switch.
- Reset / Apply buttons. Apply syncs into the URL params (TanStack Router search params) so deep-linking works.

URL is canonical state — every filter combination is deep-linkable.

## Pause UI

- Top-bar badge: `Paused (until 10:30)` (yellow). Click → resume dialog.
- When not paused: `[ Pause ]` button. Click → opens menu: indefinite / 1h / 1d.

## Export dialog

- Format radio: CSV / JSON / NDJSON.
- Filter summary (read-only): shows what's being exported.
- Estimated row count via `CountActivity`.
- Inline (download in browser) vs daemon-path (writes to local file path).
- Submit → triggers `POST /v6/activity/export`. Renders link to downloadable blob on response.

## Empty & loading states

- Empty: friendly message ("No activity in this window. Try a wider time range, or wait — mxr starts recording as you use it.") with a CTA to broaden the filter.
- Loading: skeleton rows.
- Error: red banner with retry button.

## Tests

- Unit: formatters round-trip representative context shapes.
- Component: render table with 100 seeded rows, assert column rendering and bulk selection toggles.
- e2e (Playwright): boot daemon (real, fake provider), open `/activity`, narrow filter to `action=mail.archive`, verify rows, redact one, verify update.

## Acceptance criteria

- `/activity` route is registered in TanStack Router and reachable from sidebar nav.
- All 9 bridge routes work end-to-end against a real daemon.
- Filter changes update URL params; back/forward navigation restores filters.
- Bulk redact prompts with dry-run count before mutating.
- PARITY_MATRIX updated.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Large list rendering is slow | TanStack Virtual + page size 50 + infinite query. Same pattern as the mailbox list. |
| Export of huge result tied up in browser memory | Force `inline_only=false` when count > 10k; daemon writes to disk and returns path. |
| URL-state churn from rapid filter changes | Debounce filter→URL sync by 200 ms. |
| Bundle bloat from new components | All components are shadcn-derived (already in tree) + TanStack Table. No new heavy deps. |

## Exit criteria

Phase 6 is done when:
- Activity surface in web matches TUI capability (list / filter / detail / redact / export / pause).
- Playwright e2e green.
- `docs/web-app/PARITY_MATRIX.md` updated with `Activity surface — done`.
- `STATUS.md` Phase 6 boxes ticked.
