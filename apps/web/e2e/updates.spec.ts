import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface LineAnswer {
  id: string;
  message_ids?: string[];
  tracker?: { kind: string } | null;
  section: string;
  source_key: string;
  source_name: string;
  fact: string;
  in_todo?: string | null;
  todo_suggestion?: string | null;
  delta?: { text: string } | null;
}
interface DigestAnswer {
  digest: {
    message_count: number;
    needs_a_look: LineAnswer[];
    changed: LineAnswer[];
    routine: LineAnswer[];
    selection_token: string;
    let_go_line?: string | null;
    expired: { kind: string }[];
  };
}
interface LetGoAnswer {
  result: {
    message_count: number;
    message_ids: string[];
    selection_token: string;
    line: string;
  };
}

/**
 * The demo's day of notifications is in, with the Google sign-in in
 * Needs a look (the first sync brings it).
 */
async function waitForDigest(page: Page): Promise<DigestAnswer["digest"]> {
  let digest: DigestAnswer["digest"] | null = null;
  await expect
    .poll(
      async () => {
        digest = (await bridge<DigestAnswer>(page, "/api/v1/mail/updates?expired=true")).digest;
        return digest.needs_a_look.some((line) => line.source_key.includes("google"));
      },
      { timeout: 60_000 },
    )
    .toBe(true);
  return digest!;
}

test("the digest is a briefing by source: what needs a look first, routine folded", async ({
  page,
}) => {
  const digest = await waitForDigest(page);
  // The sign-in, the failed payout and the failing build need a look.
  const needs = digest.needs_a_look.map((line) => line.source_name).join(" | ");
  expect(needs).toContain("Google");
  expect(needs).toContain("Stripe");
  expect(digest.needs_a_look.some((line) => line.tracker?.kind === "build")).toBe(true);
  // Strava's week is compared with last week's by code.
  const strava = digest.changed.find((line) => line.source_key === "strava.com");
  expect(strava?.delta?.text).toBe("up 12% on last week");
  // Four Vercel deploys are one routine line.
  expect(digest.routine.filter((line) => line.source_key === "vercel.com")).toHaveLength(1);
  // A one-time code past its ten minutes never shows.
  const shown = [...digest.needs_a_look, ...digest.changed, ...digest.routine];
  expect(shown.some((line) => /verification code/i.test(line.fact))).toBe(false);
  expect(digest.expired.some((item) => item.kind === "one-time code")).toBe(true);

  await openApp(page, "/updates");
  await expect(page.getByTestId("mode-header")).toHaveText(
    "Notifications gathered twice a day. Read the digest, then let go.",
  );
  // The page says its count once; the frame names the cut.
  await expect(page.getByTestId("updates-cut")).toHaveText(/^\d+ updates? from \d+ sources?$/);
  await expect(page.getByText(/^\d\d:\d\d digest$/)).toBeVisible();
  const needsSection = page.getByTestId("updates-needs_a_look");
  const google = needsSection.getByTestId("update-line").filter({ hasText: "Google" });
  // The sending host sits beside the display name, and nothing was added
  // to To do on arrival: the alert is a highlighted suggestion.
  await expect(google).toContainText("Google (accounts.google.com)");
  // The first line is lit, so its trail gives way to its verbs: the to-do suggestion is the `t` verb.
  await expect(google.getByRole("button", { name: /^Add to To do: / })).toBeVisible();
  await expect(google.getByRole("link")).toHaveCount(0);
  await expect(
    needsSection.getByTestId("update-line").filter({ hasText: "acme/api" }),
  ).toBeVisible();
  await expect(
    page.getByTestId("updates-changed").getByTestId("update-delta").first(),
  ).toBeVisible();
  // No email list: no subjects as rows, no unread state.
  await expect(page.getByTestId("mail-row")).toHaveCount(0);
});

test("a suggested to-do becomes one only when you add it", async ({ page }) => {
  await waitForDigest(page);
  await openApp(page, "/todo");
  const signIn = page.getByTestId("todo-row").filter({ hasText: "Check new sign-in to Google" });
  await expect(signIn).toHaveCount(0);

  await openApp(page, "/updates");
  const google = page
    .getByTestId("updates-needs_a_look")
    .getByTestId("update-line")
    .filter({ hasText: "Google" });
  await google.getByRole("button", { name: /^Add to To do: / }).click();
  await expect(google.getByTestId("update-in-todo")).toHaveText("already in To do");

  // Added by hand with no date, it waits under Whenever.
  await openApp(page, "/todo");
  await page.getByRole("button", { name: /^Whenever/ }).click();
  await expect(signIn.filter({ hasText: "accounts.google.com" })).toBeVisible();
});

test("let go of all acts on exactly the previewed cut, and u puts it back", async ({ page }) => {
  await waitForDigest(page);
  const preview = (
    await bridge<LetGoAnswer>(page, "/api/v1/mail/updates/let-go", { dry_run: true })
  ).result;
  expect(preview.message_count).toBeGreaterThan(0);

  await openApp(page, "/updates");
  await expect(page.getByTestId("update-line").first()).toBeVisible();
  await page.keyboard.press("A");
  const dialog = page.getByTestId("updates-let-go-dialog");
  await expect(dialog).toContainText(
    new RegExp(`^Let go of ${preview.message_count} updates? from`),
  );
  const done = page.waitForResponse(
    (response) =>
      response.url().endsWith("/api/v1/mail/updates/let-go") &&
      response.request().postDataJSON()?.dry_run === false,
  );
  await dialog.getByTestId("updates-let-go-confirm").click();
  const result = ((await (await done).json()) as LetGoAnswer).result;
  expect(result.message_count).toBe(preview.message_count);
  expect([...result.message_ids].sort()).toEqual([...preview.message_ids].sort());
  await expect(page.locator("[data-sonner-toast]").first()).toContainText(/^Let go of \d+/);
  // Only parcels stay: a tracker leaves Updates when it ends, not on let go.
  await expect(
    page.locator("[data-testid='update-line']:not([data-source^='parcel:'])"),
  ).toHaveCount(0);

  await page.keyboard.press("u");
  await expect
    .poll(
      async () => (await bridge<DigestAnswer>(page, "/api/v1/mail/updates")).digest.message_count,
      { timeout: 30_000 },
    )
    .toBeGreaterThan(0);
  await expect(page.getByTestId("update-line").filter({ hasText: "Strava" })).toBeVisible();
});

test("K tunes a source with undo, and e previews then lets go of one source", async ({ page }) => {
  await waitForDigest(page);
  await openApp(page, "/updates");
  const plausible = page.getByTestId("update-line").filter({ hasText: "Plausible" });
  // A hover shows a row's actions; the lamp moves on a click (and opens its peek).
  await plausible.click();
  await expect(plausible).toHaveAttribute("data-lit", "true");
  await page.keyboard.press("K");
  const tune = page.getByTestId("updates-tune-dialog");
  await tune.getByRole("button", { name: "Mute" }).click();
  await expect(page.locator("[data-sonner-toast]").first()).toContainText("Plausible: muted");
  await expect(plausible).toHaveCount(0);
  await expect(page.getByTestId("updates-hidden")).toContainText("muted");
  await page.keyboard.press("u");
  await expect(plausible).toBeVisible();

  await plausible.click();
  await page.keyboard.press("e");
  // One source previews too: the count shows before anything changes.
  const preview = page.getByTestId("updates-let-go-dialog");
  await expect(preview).toContainText("from 1 source");
  await expect(plausible).toBeVisible();
  await preview.getByTestId("updates-let-go-confirm").click();
  await expect(plausible).toHaveCount(0);
  await page.keyboard.press("u");
  await expect(plausible).toBeVisible({ timeout: 30_000 });
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("one column, no sideways scroll, Let go of all in the header and no bar over rows", async ({
    page,
  }) => {
    await waitForDigest(page);
    await openApp(page, "/updates");
    await expect(page.getByTestId("update-line").first()).toBeVisible();
    // The action lives in the header at every width; no sticky bar covers rows.
    await expect(page.getByTestId("updates-let-go-all")).toBeVisible();
    await expect(page.getByTestId("updates-let-go-all-mobile")).toHaveCount(0);
    await expect
      .poll(() =>
        page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      )
      .toBe(0);
  });
});

test("Routine is one line until Enter opens it, and it stays open for the visit", async ({
  page,
}) => {
  const digest = await waitForDigest(page);
  expect(digest.routine.length).toBeGreaterThan(0);
  await openApp(page, "/updates");
  const toggle = page.getByTestId("updates-routine-toggle");
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(toggle).toContainText(/^Routine · \d+ sources?$/);
  await expect(page.getByTestId("updates-routine")).toHaveCount(0);
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("updates-routine")).toBeVisible();
  await expect(page.getByTestId("updates-routine-toggle")).toHaveCount(0);
});

test("Space opens the details under the focused line, and Escape closes them", async ({ page }) => {
  await waitForDigest(page);
  await openApp(page, "/updates");
  await expect(page.getByTestId("update-line").first()).toBeVisible();
  await expect(page.getByTestId("update-peek")).toHaveCount(0);
  await page.keyboard.press("Space");
  const peek = page.getByTestId("update-peek");
  await expect(peek).toBeVisible();
  await expect(peek.getByTestId("update-why")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(peek).toHaveCount(0);
});

test("a peek closes with its line when it is let go, and undo does not reopen it", async ({
  page,
}) => {
  const digest = await waitForDigest(page);
  // Let go of one source needs a message to let go of, and a row the cursor can reach.
  const order = [...digest.needs_a_look, ...digest.changed];
  const target = order.find(
    (line) => (line.message_ids?.length ?? 0) > 0 && line.tracker?.kind !== "parcel",
  );
  if (!target) throw new Error("the demo has no Updates line to let go of");
  await openApp(page, "/updates");
  const line = page.locator(
    `[data-testid='update-line'][data-line-id=${JSON.stringify(target.id)}]`,
  );
  for (let at = order.indexOf(target); at > 0; at -= 1) await page.keyboard.press("j");
  await expect(line).toHaveAttribute("data-lit", "true");
  await page.keyboard.press("Space");
  await expect(line.getByTestId("update-peek")).toBeVisible();
  await page.keyboard.press("e");
  await page.getByTestId("updates-let-go-dialog").getByTestId("updates-let-go-confirm").click();
  await expect(line).toHaveCount(0);
  await expect(page.getByTestId("update-peek")).toHaveCount(0);
  await page.keyboard.press("u");
  await expect(line).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("update-peek")).toHaveCount(0);
});
