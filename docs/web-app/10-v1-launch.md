# 10 — v1 launch plan (TDD)

This doc closes out the v1 launch. Phases 1–9 built the feature surface; this plan ships it. Test-driven, vertical-slice, ~5 working days.

## Decisions (locked)

These were three open questions from the planning pass. Locked here so the implementation can move without re-asking.

1. **Interface changes** — locked as proposed:
   - `ThreadRoute` HTML mail body renders inside `<iframe srcDoc sandbox="allow-popups allow-popups-to-escape-sandbox">`. **No `allow-scripts`, no `allow-same-origin`.** The current raw-HTML-injection render path is replaced entirely.
   - `App.tsx` mounts a `QueryCache({ onError })` that routes `UnauthorizedError` to `/settings/token` and surfaces a `role="alert"` banner.
   - `AppShell` runs an unconditional `redirect("/onboarding")` when `accounts.length === 0`, regardless of current path. Today's `/m/*`-only guard goes away.

2. **Priority order**: ship in this sequence. Each phase is independently mergeable.
   1. **C1** — release workflow builds + ships SPA in the binary.
   2. **H1** — iframe sandbox + `style` attribute stripped from sanitizer.
   3. **C2 + C3** — onboarding redirect + 401 banner.
   4. **H3** — five Playwright specs (mutation, compose, search, WS, onboarding).
   5. **H2** — tracker pixel stripping.
   6. **H4** — accessible names on icon buttons + axe full-severity pass.
   7. **H5 + H7 + H8** — sync banner, per-route error boundaries, offline banner.
   8. **C4 + H6** — README quickstart, http-bridge guide for `--remote-host`.

3. **Stop conditions**: if any Playwright spec in Phase 4 (H3) reveals a real bug in the mutation / compose / search / WS code, **stop and fix the bug before continuing**. The safety net only works if we react when it lights up. Don't push past red.

## Workflow rules (TDD skill)

- **Vertical slices**: one test → one impl → next. Never write all tests first.
- **Quality gate per RED test** (five checks below must all pass before writing GREEN code):
  1. Could the implementation be completely rewritten with a different internal design and this test still pass?
  2. Would the test fail if I deleted the function body? (i.e., not tautological)
  3. Are expected values from the spec, not from reading the implementation?
  4. Does the test exercise only the public interface (no internal mocks, no call-count assertions)?
  5. Distinct equivalence class — not redundant with an existing test?
- **Never refactor while RED.** Get to GREEN first.
- **Tests are committed at GREEN.** Squash refactor commits afterward.

---

## Phase 0 — Harness readiness (half day, no TDD)

Before any RED cycle, the test harness must boot:

1. `cd apps/web && node scripts/e2e-server.mjs --once` — confirms the fake-provider daemon spawns, the bridge serves, the SPA dev server connects.
2. `npm run test` — Vitest discovers the empty test tree without erroring.
3. `npx playwright test --list` — Playwright finds the spec files.

If any step fails, fix the harness first. The rest of the plan depends on it.

---

## Phase 1 — C1 release workflow + CI smoke (~1 day)

**Not TDD-natural** (CI YAML). Verified by behavior at the end.

### Acceptance behavior

A binary produced by the GitHub release workflow, launched with `mxr web --no-open --print-url`, serves a non-placeholder SPA at `/`.

### CI smoke step (the "test")

```yaml
- name: SPA embedded in release binary
  run: |
    ./target/release/mxr daemon --foreground &
    DAEMON=$!
    sleep 2
    URL=$(./target/release/mxr web --no-open --print-url)
    sleep 2
    HTML=$(curl -fsSL "$URL" || echo FAIL)
    echo "$HTML" | grep -q 'spa-not-built' && { echo "FAIL: placeholder served"; exit 1; }
    echo "$HTML" | grep -q '/assets/index-' || { echo "FAIL: no Vite-built bundle"; exit 1; }
    kill $DAEMON
```

**Quality-gate check**: would fail if `npm run build` step were removed (real signal). Expected values are from the placeholder HTML marker + the Vite hashed-asset naming convention — both spec-derived, not implementation-guessed.

### Implementation

`.github/workflows/release.yml`:
- Add `actions/setup-node@v6` before the cargo build.
- Add `cd apps/web && npm ci && npm run build` step.
- Append `--features web-ui` to every cargo build invocation in the matrix.
- Add the smoke step above as a final job in the same workflow.

---

## Phase 2 — H1 HTML email hardening (~1 day, pure TDD)

Highest security/UX payoff in the plan. All cycles in this phase are Vitest unit tests against `src/lib/sanitizeHtml.ts` plus one Playwright cycle for the iframe.

### Cycle 2.1 — Inline `<script>` is stripped

`src/lib/sanitizeHtml.test.ts`:
```ts
it("strips inline <script> tags", () => {
  const dirty = `<p>hi</p><script>window.X=1</script>`;
  const clean = sanitizeHtml(dirty);
  expect(clean).not.toMatch(/<script/i);
  expect(clean).not.toMatch(/window\.X/);
});
```
**Implementation**: confirm existing config catches this.

### Cycle 2.2 — `style` attribute is removed entirely (CSS exfil)

```ts
it("removes style attributes to prevent CSS exfiltration", () => {
  const dirty = `<p style="background:url(http://evil/leak)">hi</p>`;
  const clean = sanitizeHtml(dirty);
  expect(clean).not.toMatch(/style=/i);
  expect(clean).not.toMatch(/evil/);
});
```
**Implementation**: drop `"style"` from `ALLOWED_ATTR` in `sanitizeHtml.ts`.

### Cycle 2.3 — Sandboxed iframe rendering

Playwright `e2e/html-rendering.spec.ts`:
```ts
test("HTML body renders inside a sandboxed iframe", async ({ page }) => {
  await daemon.seedMessage({
    subject: "smoke",
    html: `<script>window.parent.PWNED=1</script><p>visible body</p>`,
  });
  await page.goto("/m/inbox");
  await page.getByText("smoke").click();

  const frame = page.locator("iframe[sandbox]").first();
  await expect(frame).toBeVisible();
  await expect(frame).toHaveAttribute("sandbox", /allow-popups/);
  await expect(frame).not.toHaveAttribute("sandbox", /allow-scripts/);
  expect(await page.evaluate(() => (window as { PWNED?: number }).PWNED)).toBeUndefined();
  await expect(frame.contentFrame().getByText("visible body")).toBeVisible();
});
```
**Quality-gate**: asserts the user-visible & security-visible outcome (script not executed, body still readable). Survives a rewrite from `srcDoc` to `Blob` URL iframe to MessageChannel-style render.

**Implementation**:
- `src/features/thread/MessageBody.tsx` (new) — receives raw HTML + content-type; passes through `sanitizeHtml`; renders `<iframe srcDoc={sanitized} sandbox="allow-popups allow-popups-to-escape-sandbox" />`.
- Auto-height via `ResizeObserver` on `contentDocument.body`.
- `ThreadRoute.tsx` — swap the existing raw-HTML render block for `<MessageBody html={...} />`.

---

## Phase 3 — C2 + C3 first-launch UX (~half day, pure TDD)

Two behaviors, two Playwright cycles.

### Cycle 3.1 — Empty-accounts user lands on onboarding from any URL

`e2e/onboarding-redirect.spec.ts`:
```ts
test("blank install lands on onboarding from any URL", async ({ page }) => {
  await daemon.resetAccounts();
  for (const path of ["/", "/m/inbox", "/analytics", "/rules"]) {
    await page.goto(path);
    await expect(page).toHaveURL(/\/onboarding$/);
  }
});
```
**Implementation**: in `AppShell.tsx`, when `accounts.data?.accounts?.length === 0`, navigate to `/onboarding` unconditionally — current `/m/*`-only guard removed.

### Cycle 3.2 — 401 surfaces an alert and routes to /settings/token

`e2e/token-expiry.spec.ts`:
```ts
test("401 routes to /settings/token with a visible alert", async ({ page }) => {
  await page.goto("/m/inbox");
  await expect(page.getByRole("article").first()).toBeVisible();

  await page.evaluate(() => localStorage.setItem("mxr.bridgeToken", "invalid-token"));
  await page.reload();

  await expect(page).toHaveURL(/\/settings\/token/);
  await expect(page.getByRole("alert")).toContainText(/token/i);
});
```
**Implementation**:
- `App.tsx` — replace bare `QueryClientProvider` with one that mounts:
  ```ts
  new QueryCache({
    onError: (err) => {
      if (err instanceof UnauthorizedError) router.navigate({ to: "/settings/token", search: { reason: "expired" } });
    },
  })
  ```
- `routes/settings.$section.tsx` — accept search param `reason`; pass to `TokenSection`.
- `TokenSection.tsx` — render a `role="alert"` banner when `reason === "expired"`.

---

## Phase 4 — H3 mutation + realtime safety net (~1 day, pure TDD via Playwright)

Five end-to-end specs. **If any reveals a real bug, stop and fix it before moving on.**

### Cycle 4.1 — Archive then undo restores the row

```ts
test("archive then undo restores the message", async ({ page }) => {
  await daemon.seedInbox(3);
  await page.goto("/m/inbox");
  const rows = page.getByRole("article");
  await expect(rows).toHaveCount(3);
  const subject = await rows.first().getByRole("heading").textContent();

  await rows.first().getByLabel(/archive/i).click();
  await expect(rows).toHaveCount(2);

  await page.getByRole("button", { name: /undo/i }).click();
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: subject! })).toBeVisible();
});
```

### Cycle 4.2 — Compose → send → appears in Sent

```ts
test("send draft appears in Sent folder", async ({ page }) => {
  await page.goto("/compose/new");
  await page.getByLabel(/^to$/i).fill("alice@example.com");
  await page.getByLabel(/subject/i).fill("smoke-send");
  await page.locator(".cm-content").fill("body");
  await page.keyboard.press("Meta+Enter");
  await page.getByRole("button", { name: /^send$/i }).click();

  await page.goto("/m/sent");
  await expect(page.getByText("smoke-send")).toBeVisible();
});
```

### Cycle 4.3 — Search produces live results

```ts
test("typing in top-bar search shows live results", async ({ page }) => {
  await daemon.seedMessages([{ subject: "Q4 plan" }, { subject: "Q3 plan" }]);
  await page.goto("/m/inbox");
  await page.keyboard.press("/");
  await page.keyboard.type("Q4");
  await expect(page.getByRole("option").filter({ hasText: "Q4 plan" })).toBeVisible();
});
```

### Cycle 4.4 — WS disconnect surfaces, reconnect clears

```ts
test("WS disconnect surfaces reconnecting state", async ({ page }) => {
  await page.goto("/m/inbox");
  await daemon.stop();
  await expect(page.getByText(/reconnecting|offline/i)).toBeVisible({ timeout: 6_000 });
  await daemon.restart();
  await expect(page.getByText(/^connected$/i)).toBeVisible({ timeout: 6_000 });
});
```

### Cycle 4.5 — Onboarding redirect (covered by Cycle 3.1)

Skip — already green.

---

## Phase 5 — H2 tracker pixel stripping (~half day, pure TDD)

### Cycle 5.1 — 1×1 pixel is removed when remote images allowed

`src/lib/sanitizeHtml.test.ts`:
```ts
it("removes 1x1 tracking pixels even with remote images on", () => {
  const dirty = `<p>Newsletter</p><img src="https://t.example.com/p.gif" width="1" height="1">`;
  const clean = sanitizeHtml(dirty, { allowRemoteImages: true });
  expect(clean).not.toMatch(/p\.gif/);
});
```

### Cycle 5.2 — Normal product image passes through (regression guard)

```ts
it("keeps normal-sized product images", () => {
  const dirty = `<img src="https://cdn.example.com/hero.jpg" width="600" height="400" alt="hero">`;
  const clean = sanitizeHtml(dirty, { allowRemoteImages: true });
  expect(clean).toMatch(/hero\.jpg/);
  expect(clean).toMatch(/alt="hero"/);
});
```

### Cycle 5.3 — Known tracker domain is removed regardless of size

```ts
it("removes known tracker-domain images regardless of size", () => {
  const dirty = `<img src="https://mailtrack.io/trace/abc" width="600" height="400">`;
  const clean = sanitizeHtml(dirty, { allowRemoteImages: true });
  expect(clean).not.toMatch(/mailtrack/);
});
```

**Implementation**: in `sanitizeHtml.ts`'s `afterSanitizeAttributes` hook, for `<img>` nodes:
- Remove if `width ≤ 2` OR `height ≤ 2`.
- Remove if `src` host matches one of: `mailtrack.io`, `track.customer.io`, `email.mg.*`, `sendgrid.net/wf/open`, `mandrillapp.com/track`. Keep the list tight; new entries via PR.

---

## Phase 6 — H4 accessibility (~half day, pure TDD)

### Cycle 6.1 — Every icon-only button has an accessible name

`e2e/accessibility.spec.ts` adds:
```ts
test("every icon-only button has an accessible name", async ({ page }) => {
  await page.goto("/m/inbox");
  const buttons = await page.getByRole("button").all();
  for (const btn of buttons) {
    const label = (await btn.getAttribute("aria-label"))?.trim()
      ?? (await btn.textContent())?.trim();
    expect(label, `button without accessible name: ${await btn.innerHTML()}`).toBeTruthy();
  }
});
```
**Implementation**: walk Topbar, BulkActionBar, ThreadActionsToolbar, AccountSwitcher, MoreActionsMenu, ConnectionPill, RightRail close button. Add `aria-label` to every icon-only button.

### Cycle 6.2 — axe-core reports zero violations across severities

`e2e/accessibility.spec.ts`:
```ts
const ROUTES = ["/m/inbox", "/compose/new", "/search?q=test", "/settings/theme", "/onboarding"];
for (const path of ROUTES) {
  test(`axe-core: no violations on ${path}`, async ({ page }) => {
    await page.goto(path);
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
}
```
**Implementation**: drop the existing severity filter in the current spec. Fix violations one route at a time, vertical-slice style.

---

## Phase 7 — H5 + H7 + H8 resilience UX (~half day, pure TDD)

### Cycle 7.1 — Sync progress banner while syncing

```ts
test("syncing shows a progress banner with counter", async ({ page }) => {
  await daemon.startInitialSync({ totalMessages: 2_000, pacedMs: 50 });
  await page.goto("/m/inbox");
  await expect(page.getByRole("status").filter({ hasText: /sync/i })).toBeVisible();
  await expect(page.getByText(/\d+\s+(?:of|\/)\s*~?\d+/)).toBeVisible();
  await daemon.completeSync();
  await expect(page.getByRole("status").filter({ hasText: /sync/i })).toBeHidden({ timeout: 4_000 });
});
```
**Implementation**: a `<SyncProgressBanner>` component reading from `useConnectionStore.syncProgress`; mounted at the top of `MailboxRoute`. Sticky inside the mailbox column.

### Cycle 7.2 — Per-route error boundary keeps shell intact

```ts
test("a thrown render error in thread route doesn't blank the shell", async ({ page }) => {
  await page.goto("/m/inbox/not-a-real-thread-id-12345");
  await expect(page.getByRole("navigation", { name: /sidebar/i })).toBeVisible();
  await expect(page.getByRole("alert")).toContainText(/error|thread/i);
});
```
**Implementation**: register an `errorComponent` on every route via TSR — single shared `<RouteError />` that renders the error message with a retry button. Per-route, not just app-shell-wide.

### Cycle 7.3 — Sticky offline banner after 30s

```ts
test("WS offline > 30s shows sticky offline banner", async ({ page }) => {
  await page.goto("/m/inbox");
  await daemon.stop();
  await expect(page.locator("[data-offline-banner]")).toBeVisible({ timeout: 32_000 });
  await daemon.restart();
  await expect(page.locator("[data-offline-banner]")).toBeHidden({ timeout: 5_000 });
});
```
**Implementation**: `<OfflineBanner>` reads `connectionStore.state` + a derived "offline-for-ms" computed locally. Renders when state has been `offline`/`reconnecting` for > 30s. Mounted in `AppShell`.

---

## Phase 8 — C4 + H6 docs (~half day, no TDD)

Direct edits; verification = "junior engineer can follow this without asking questions."

- **Root `README.md`** — add a "Web interface" section between "Quick start" and "Architecture":
  ```
  ### Web interface

  After running `mxr setup` and `mxr daemon`, launch the browser-based interface with:

  $ mxr web

  This opens your default browser to http://mxr.localhost:42829. Local auth uses the same-machine `/api/v1/auth/local-token` handshake, so the launch URL does not carry the bridge token.

  Flags:
  - `--port N`        bind to a different fixed local port
  - `--auto-port`     try the next free port on conflict
  - `--no-open`       just print the URL, do not open browser
  - `--remote-host H` open browser to a manually configured remote bridge at H
  - `--print-url`     print the URL and continue serving

  Troubleshooting:
  - Blank page: confirm the daemon is running (`mxr status`).
  - 401 / unauthorized: the SPA will redirect you to /settings/token; paste the token from ~/.config/mxr/bridge-token.
  - Stale UI after upgrading: hard-refresh (Cmd-Shift-R / Ctrl-F5).
  ```

- **`docs/guides/http-bridge.md`** — append a "Remote access" section:
  - Default recommendation: keep the bridge loopback-bound and use SSH/Tailscale/WireGuard.
  - Manual remote-host mode: bridge behind a TLS reverse proxy; CORS + Host allowlist configured in `[bridge]`.
  - Token setup: write the remote bridge token to `~/.config/mxr/bridge-tokens/<host>.token` (mode 0600) on the client machine.
  - Launch: `mxr web --remote-host mxr.example.com`.

- **`docs/web-app/STATUS.md`** — mark all v1 launch items done as Phase 8 lands.

---

## Per-cycle checklist

Apply this to every RED test before writing GREEN code:

- [ ] Test describes behavior, not implementation
- [ ] Test uses public interface only (no `useStore.getState()`, no exported internal helpers)
- [ ] Test would survive a complete implementation rewrite
- [ ] Test does NOT assert on internal method calls, call counts, or call order
- [ ] Expected values are literals from spec, not computed from implementation
- [ ] Test would fail if function body was deleted (not tautological)
- [ ] Test covers a distinct equivalence class (not redundant)
- [ ] Edge cases included (boundary, empty, error paths) where natural
- [ ] No speculative features added in GREEN code
- [ ] Refactor only after GREEN, never during RED

---

## Total timeline

| Day | Phase | Output |
|---|---|---|
| 1 | 1 | Release workflow + CI smoke step shipping SPA in binary |
| 2 am | 2 | Iframe sandbox + sanitizer hardened (H1) |
| 2 pm | 3 | Onboarding redirect + 401 banner (C2, C3) |
| 3 | 4 | Five Playwright specs (H3) — STOP if real bugs surface |
| 4 am | 5 | Tracker pixel stripping (H2) |
| 4 pm | 6 | Accessible names + axe full-severity pass (H4) |
| 5 am | 7 | Sync banner + error boundaries + offline banner (H5, H7, H8) |
| 5 pm | 8 | Docs (C4, H6) + cut tag, install fresh, smoke |

---

## Where to start

```bash
cd /Users/bhekanik/code/planetaryescape/mxr/apps/web
node scripts/e2e-server.mjs --once   # Phase 0 harness check
```

If that exits 0, the next file to open is `.github/workflows/release.yml` (Phase 1).

If the harness check fails, fix the harness before touching anything else.

---

## Out of scope for v1

Listed here so a future session doesn't relitigate:

- Vitest unit tests beyond the sanitizer (Phase 4 Playwright + real daemon gives more coverage per hour).
- Web Push (post-v1).
- Mobile / phone responsive (≥768 px tablet is the floor).
- Wrapped story-mode polish, share-as-image (STATUS-tracked).
- Account-scoped invalidation refinement (refetch papers over it).
- Notification deduplication on reconnect (rare, mild duplication acceptable).
- Compose drafts schema versioning (add a `version: 1` field as preventive measure if convenient; otherwise defer).
