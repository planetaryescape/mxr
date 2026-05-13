# Phase 6 — Analytics (six dashboards + Wrapped story+dashboard)

Goal: ship six analytics dashboards with a shared chrome. Each is its own page. Wrapped ships in two view modes — story (Spotify-Wrapped style with share-as-image) and dashboard (sober charts) — user-toggleable.

## Deliverables

1. `/analytics` index — quick links to each dashboard.
2. `/analytics/storage` — storage breakdown (sender / mimetype / label) + largest messages list.
3. `/analytics/stale` — stale threads (mine / theirs) with thresholds.
4. `/analytics/contacts` — asymmetry + decay views.
5. `/analytics/response-time` — percentile clock + counterparty filter + business-hours mode.
6. `/analytics/subscriptions` — ROI ranking with in-place unsubscribe.
7. `/analytics/wrapped` — story + dashboard modes, toggleable.
8. **Shared chrome**: title, date-range selector (7d/30d/90d/1y/custom), account filter, "Rebuild" trigger, mode toggles per page.
9. **Charts**: simple line/bar via Recharts. Hover dims non-hovered bars; tooltip shows real values.
10. **Drill-downs**: open a side panel in the right rail (not modals).

## Bridge endpoints used

- `GET /api/v1/platform/analytics/storage-breakdown?group_by=&limit=`
- `GET /api/v1/platform/analytics/largest-messages?limit=&since_days=`
- `GET /api/v1/platform/analytics/stale-threads?perspective=mine|theirs&older_than_days=&within_days=&limit=`
- `GET /api/v1/platform/analytics/contact-asymmetry?limit=`
- `GET /api/v1/platform/analytics/contact-decay?limit=`
- `GET /api/v1/platform/analytics/response-time?direction=mine|theirs&counterparty=`
- `GET /api/v1/platform/subscriptions?rank=true&limit=`
- `GET /api/v1/platform/analytics/wrapped?since_unix=&until_unix=&label=`
- `POST /api/v1/platform/analytics/rebuild`

The wrapped endpoint returns a payload shaped like:
```ts
{
  window: { type: "ytd" | "year" | "since_days", value: number };
  totals: { sent: number, received: number, threads: number };
  top_contacts: Array<{ email: string, count: number }>;
  time_patterns: { busiest_hour: number, busiest_day: string, ... };
  reply_discipline: { p50_minutes: number, p90_minutes: number, ... };
  storage: { total_bytes: number, by_label: ... };
  newsletters: { total: number, top: ... };
  superlatives: Array<{ label: string, value: any }>;
}
```
(Verify in generated.ts.)

## Files

```
src/features/analytics/
  AnalyticsLayout.tsx              # shared chrome: header, range, account filter
  AnalyticsRoute.tsx               # /analytics index
  charts/
    LineChart.tsx                  # Recharts wrapper with our theme
    BarChart.tsx
    StatCard.tsx                   # big-number tile
    SparkLine.tsx
  storage/
    StorageRoute.tsx
    StorageBreakdown.tsx           # bar chart
    LargestMessagesList.tsx
  stale/
    StaleRoute.tsx
    StaleControls.tsx              # mine/theirs toggle, thresholds
  contacts/
    ContactsRoute.tsx
    AsymmetryView.tsx
    DecayView.tsx
  responseTime/
    ResponseTimeRoute.tsx
    PercentileClock.tsx            # inverted radial / clock viz
    CounterpartyFilter.tsx
  subscriptions/
    SubscriptionsRoute.tsx
    SubscriptionRow.tsx            # in-place unsubscribe button
  wrapped/
    WrappedRoute.tsx
    WrappedModeToggle.tsx          # story / dashboard switch
    story/
      StoryShell.tsx               # full-bleed slide container
      StorySlideTotal.tsx
      StorySlideTopContacts.tsx
      StorySlideTimePatterns.tsx
      StorySlideReplyDiscipline.tsx
      StorySlideStorage.tsx
      StorySlideNewsletters.tsx
      StorySlideSuperlatives.tsx
      ShareAsImageButton.tsx       # html2canvas → png
    dashboard/
      WrappedDashboard.tsx
      WrappedTotalsCard.tsx
      WrappedTopContactsList.tsx
      ...
src/components/
  AnalyticsRangePicker.tsx
  AccountFilter.tsx
  AnalyticsRebuildButton.tsx
```

## Wrapped story mode

- Full-bleed (escapes the AppShell main pane padding).
- Pagination via PageUp/PageDown, ←/→ keys, or swipe (touchpad).
- Each slide a dramatic stat with hero typography (text-3xl / text-4xl from tokens).
- Background gradients pulled from theme tokens (varied per slide for variety).
- Subtle CSS animations: number count-up on slide enter, fade-in for hero text.
- "Share this slide as image" button per slide → `html2canvas` (or native View Transitions + `<canvas>` fallback) generates a PNG and triggers download.
- Color story switches per slide: chart-1 → chart-6 token rotation.

## Wrapped dashboard mode

- Same data, sober charts. Same chrome as the other dashboards.
- 2-column grid of stat cards + line/bar charts.
- Each row = a category (volume, time, contacts, reply, storage, newsletters).

## Recharts theme

- Pull color from `getCSSVar('--chart-1')` etc. Recharts accepts string colors; convert HSL to `hsl(var(--chart-1))`.
- Grid lines `hsl(var(--border))`.
- Tooltips: custom component to match our popover style.
- Disable default axis ticks; use ours via `tickFormatter`.

## Drill-downs

- Click a row in Storage → right rail opens "Top sender: alice@…" with their thread list and bulk action ability ("trash all >1 MB", "archive all").
- Click a stale thread → right rail opens with thread peek + actions.
- All drill-downs URL-driven so deep linking works: `?drill=sender:alice@example.com`.

## Verification

1. `/analytics/storage` → bar chart renders, hover dims others, tooltip shows bytes.
2. Change range → query refetches, chart re-renders.
3. Click a sender bar → right rail opens with messages from that sender.
4. `/analytics/stale?mine` → list of threads I owe replies on, ordered by age.
5. `/analytics/wrapped?year=2025` → story mode by default; toggle → dashboard mode.
6. Story mode: arrow keys navigate slides; share button downloads PNG.
7. Subscriptions: Unsubscribe row → confirms → POST to bridge → row updates.

## Decisions

- 2026-05-10 — `html2canvas` for share-as-image. View Transitions is nice but PNG export needs canvas; fallback path covers older browsers.
- 2026-05-10 — Wrapped story slides defined as a registry; new slides are easy to add. Each slide a small component receiving the wrapped payload.
