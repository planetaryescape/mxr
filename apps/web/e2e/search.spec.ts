import { expect, test } from "@playwright/test";

import { mailList, mailRows, openList, pressSequence } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

function query(page: { url: () => string }): string {
  return new URL(page.url()).searchParams.get("q") ?? "";
}

test("operator chips show each token and removing one rewrites the query", async ({ page }) => {
  await openList(page, "/search?q=canary%20is:unread");
  const chip = page.getByRole("button", { name: /^Remove is:\s?unread/ });
  await expect(chip).toBeVisible();
  await expect(page.getByRole("button", { name: /^Remove canary/ })).toBeVisible();

  await chip.click();
  await expect.poll(() => query(page)).toBe("canary");
  await expect(page.getByLabel("Search query")).toHaveValue("canary");
  await expect(page.getByRole("button", { name: /^Remove is:\s?unread/ })).toHaveCount(0);
});

test("clicking a sender facet narrows the query and the results", async ({ page }) => {
  await openList(page, "/search?q=canary");
  const facet = page.locator('button[title*=" unread"]').first();
  await expect(facet).toBeVisible();
  const facetCount = Number((await facet.locator("span").last().textContent())?.trim());
  const before = await mailRows(page).count();

  await facet.click();
  await expect.poll(() => query(page)).toMatch(/^canary from:\S+/);
  await expect(page.getByRole("button", { name: /^Remove from:/ })).toBeVisible();
  await expect(mailRows(page).first()).toBeVisible();
  const after = await mailRows(page).count();
  expect(after).toBeLessThanOrEqual(Math.min(before, facetCount));
});

// BUG: POST /api/v1/platform/saved-searches/create answers 502 "unexpected
// response from daemon": the bridge (crates/web/src/lib.rs
// create_saved_search) expects an Ack but the daemon replies SavedSearchData.
// The dialog stays open with "Couldn't save the search".
test.fixme("save a search, find it in the sidebar, and jump to it with g 1", async ({ page }) => {
  const name = `E2E canary ${Date.now().toString(36)}`;
  await openList(page, "/search?q=canary");

  await page.getByRole("button", { name: "Save", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Save this search" });
  await dialog.getByLabel("Name").fill(name);
  await dialog.getByRole("button", { name: "Save search" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText(`Saved “${name}”`)).toBeVisible();

  const sidebarLink = page.getByRole("complementary", { name: "Mailboxes" }).getByRole("link", {
    name: new RegExp(`^${name}`),
  });
  await expect(sidebarLink).toBeVisible();

  // Leave for Inbox, then jump back with the saved-search chord.
  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(mailList(page)).toBeVisible();
  await pressSequence(page, "g", "1");
  await expect(page).toHaveURL(/\/m\/saved\/[^/]+$/);
  await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
  await expect(mailRows(page).first()).toBeVisible();
});

test("g 1 with no saved search in that slot says so and stays put", async ({ page }) => {
  await openList(page, "/m/inbox");
  await pressSequence(page, "g", "1");
  await expect(page.getByText("No saved search in slot 1")).toBeVisible();
  await expect(page).toHaveURL(/\/m\/inbox$/);
});
