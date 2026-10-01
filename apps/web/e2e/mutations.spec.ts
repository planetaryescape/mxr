import { expect, test } from "@playwright/test";

import { mailRows, openList, rowById } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

test("hover archive removes the row at once and the toast's Undo restores it", async ({ page }) => {
  await openList(page, "/m/inbox");
  const row = mailRows(page).first();
  const rowId = (await row.getAttribute("id"))!;

  // Hold the response so the optimistic removal is what the user sees.
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/v1/mail/mutations/archive", async (route) => {
    await held;
    await route.continue();
  });

  await row.hover();
  const archiveResponse = page.waitForResponse("**/api/v1/mail/mutations/archive");
  await row.getByTitle("Archive (e)").click();
  await expect(rowById(page, rowId)).toHaveCount(0);
  release();
  const response = await archiveResponse;
  expect(response.ok(), await response.text()).toBe(true);
  const body = (await response.json()) as { result?: { mutation_id?: string } };
  expect(body.result?.mutation_id).toBeTruthy();

  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(rowById(page, rowId)).toBeVisible();
});

test("a failed archive puts the row back and says why", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = (await mailRows(page).first().getAttribute("id"))!;
  await page.route("**/api/v1/mail/mutations/archive", (route) =>
    route.fulfill({
      status: 409,
      contentType: "application/json",
      json: { code: "conflict", error: "provider rejected the change" },
    }),
  );

  await page.keyboard.press("e");
  await expect(page.getByText(/failed/i).first()).toBeVisible();
  await expect(rowById(page, rowId)).toBeVisible();
});

test("archive with e then z undoes via the global shortcut", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = (await mailRows(page).first().getAttribute("id"))!;

  const archiveResponse = page.waitForResponse("**/api/v1/mail/mutations/archive");
  await page.keyboard.press("e");
  expect((await archiveResponse).ok()).toBe(true);
  await expect(rowById(page, rowId)).toHaveCount(0);

  const undoResponse = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("z");
  expect((await undoResponse).ok()).toBe(true);
  await expect(rowById(page, rowId)).toBeVisible();
});

test("an archive that partly fails keeps `u` for the part that changed", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.getByTestId("mailbox-list").focus();
  const rowId = (await mailRows(page).first().getAttribute("id"))!;
  // The daemon archives it, but the answer reports a second message that
  // failed (the daemon keeps the undo for what changed, as it does for a
  // real partial failure): the request is real, only its count is not.
  await page.route("**/api/v1/mail/mutations/archive", async (route) => {
    const response = await route.fetch();
    const body = (await response.json()) as {
      result: { requested: number; succeeded: number; failed: number; mutation_id?: string };
    };
    expect(body.result.mutation_id).toBeTruthy();
    body.result.requested += 1;
    body.result.failed += 1;
    await route.fulfill({ response, json: body });
  });

  await page.keyboard.press("e");
  await expect(page.getByText(/; the rest failed$/).first()).toBeVisible();
  await expect(page.getByText(/Press u to undo what changed\.$/).first()).toBeVisible();
  await expect(rowById(page, rowId)).toHaveCount(0);

  const undoResponse = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("u");
  expect((await undoResponse).ok()).toBe(true);
  await expect(rowById(page, rowId)).toBeVisible();
});
