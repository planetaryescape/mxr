import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });
test.describe.configure({ timeout: 90_000 });

interface LineAnswer {
  id: string;
  message_ids?: string[];
  tracker?: { kind: string } | null;
  section: string;
  source_key: string;
  source_name: string;
  fact: string;
  unread?: boolean;
  seen_revision?: string;
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
  await expect(page.getByTestId("update-line").first()).toBeVisible({ timeout: 30_000 });
  const firstRow = page.locator('[data-testid="update-line"][data-unread="true"]').first();
  const rowId = await firstRow.getAttribute("data-line-id");
  const seen = page.waitForResponse(
    (response) =>
      response.url().endsWith("/api/v1/mail/rows/seen") && response.request().method() === "POST",
  );
  await firstRow.getByRole("button", { name: "peek", exact: true }).click();
  expect((await seen).ok()).toBe(true);
  const persisted = (await bridge<DigestAnswer>(page, "/api/v1/mail/updates")).digest;
  expect(
    [...persisted.needs_a_look, ...persisted.changed, ...persisted.routine].find(
      (line) => line.id === rowId,
    )?.unread,
  ).toBe(false);
  expect(persisted.message_count).toBe(full);
  await page.reload();
  await expect(
    page.locator(`[data-testid="update-line"][data-line-id="${rowId}"]`),
  ).not.toHaveAttribute("data-unread", "true");
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
  await expect(
    page.locator("[data-testid='update-line']:not([data-source^='parcel:'])"),
  ).toHaveCount(0);

  await page.keyboard.press("u");
  await expect
    .poll(
      async () => (await bridge<DigestAnswer>(page, "/api/v1/mail/updates")).digest.message_count,
      { timeout: 30_000 },
    )
    .toBe(full);
  await expect(page.getByTestId("update-line").filter({ hasText: "Strava" })).toBeVisible();
});
