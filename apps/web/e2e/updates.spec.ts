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

test("let go of all acts on exactly the previewed cut, and u puts it back", async ({ page }) => {
  await waitForDigest(page);
  const preview = (
    await bridge<LetGoAnswer>(page, "/api/v1/mail/updates/let-go", { dry_run: true })
  ).result;
  expect(preview.message_count).toBeGreaterThan(0);

  const full = (await bridge<DigestAnswer>(page, "/api/v1/mail/updates")).digest.message_count;
  await openApp(page, "/updates");
  await expect(page.getByTestId("update-line").first()).toBeVisible();
  await page.keyboard.press("E");
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
  // Undo puts the messages back over about two seconds (the digest grows from
  // a few to all of them), so wait for every one, not just the first, before
  // asking the page: CI read it mid-restore and failed.
  await expect
    .poll(
      async () => (await bridge<DigestAnswer>(page, "/api/v1/mail/updates")).digest.message_count,
      { timeout: 30_000 },
    )
    .toBe(full);
  await expect(page.getByTestId("update-line").filter({ hasText: "Strava" })).toBeVisible();
});
