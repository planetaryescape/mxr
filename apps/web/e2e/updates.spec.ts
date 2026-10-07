import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface LineAnswer {
  id: string;
  section: string;
  source_key: string;
  source_name: string;
  fact: string;
  in_todo?: string | null;
  delta?: { text: string } | null;
  tracker?: { kind: string } | null;
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
 * The demo's day of notifications is in and the Google sign-in broke
 * through to To do (both happen on the first sync).
 */
async function waitForDigest(page: Page): Promise<DigestAnswer["digest"]> {
  let digest: DigestAnswer["digest"] | null = null;
  await expect
    .poll(
      async () => {
        digest = (await bridge<DigestAnswer>(page, "/api/v1/mail/updates?expired=true")).digest;
        return digest.needs_a_look.find((line) => line.source_key.includes("google"))?.in_todo;
      },
      { timeout: 60_000 },
    )
    .toBe("already in To do");
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
  await expect(page.getByTestId("updates-cut")).toContainText(
    /digest · \d\d:\d\d · \d+ updates? from/,
  );
  const needsSection = page.getByTestId("updates-needs_a_look");
  const google = needsSection.getByTestId("update-line").filter({ hasText: "Google" });
  await expect(google.getByTestId("update-in-todo")).toHaveText("already in To do");
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

test("the sign-in that broke through is in To do", async ({ page }) => {
  await waitForDigest(page);
  await openApp(page, "/todo");
  await expect(
    page.getByTestId("todo-row").filter({ hasText: "Check new sign-in to Google" }),
  ).toBeVisible();
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
  await plausible.hover();
  await page.keyboard.press("K");
  const tune = page.getByTestId("updates-tune-dialog");
  await tune.getByRole("button", { name: "Mute" }).click();
  await expect(page.locator("[data-sonner-toast]").first()).toContainText("Plausible: muted");
  await expect(plausible).toHaveCount(0);
  await expect(page.getByTestId("updates-hidden")).toContainText("muted");
  await page.keyboard.press("u");
  await expect(plausible).toBeVisible();

  await plausible.hover();
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

  test("one column, no sideways scroll, Let go of all at the thumb", async ({ page }) => {
    await waitForDigest(page);
    await openApp(page, "/updates");
    await expect(page.getByTestId("update-line").first()).toBeVisible();
    await expect(page.getByTestId("updates-let-go-all-mobile")).toBeVisible();
    await expect(page.getByTestId("updates-let-go-all")).toBeHidden();
    await expect
      .poll(() =>
        page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      )
      .toBe(0);
  });
});
