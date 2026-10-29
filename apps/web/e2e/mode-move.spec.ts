import { expect, test } from "@playwright/test";

import { cursorTo, mailList, openList, rowById } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

/*
 * A mode move (X) goes through the daemon: a second client watching the
 * same list sees the move and its undo over the event stream.
 */

test("X moves an email, every client sees it, and u puts it back", async ({ page, browser }) => {
  // A second client watching Inbox: the move reaches it through the daemon.
  const other = await browser.newPage();
  await openList(other, "/m/inbox");

  await openList(page, "/m/inbox");
  await mailList(page).focus();
  const rowId = await cursorTo(page, () => true);
  // The lit row shows its verbs in place of its marks; the mark stays in the row, and the
  // status bar carries where it went.
  const chip = rowById(page, rowId).getByTestId("mode-chip");
  await expect(chip).toBeAttached();
  const before = (await chip.getAttribute("data-bucket")) ?? "";
  const [key, after] = before === "reading" ? ["u", "updates"] : ["r", "reading"];

  await page.keyboard.press("X");
  const picker = page.getByTestId("move-to-mode-dialog");
  await expect(picker).toBeVisible();
  await page.keyboard.press(key);
  await expect(picker).toHaveCount(0);
  await expect(page.locator("[data-sonner-toast]").filter({ hasText: /^Moved to / })).toBeVisible();
  await expect(chip).toHaveAttribute("data-bucket", after);
  await expect(chip).toHaveAttribute("title", /you moved this email/);
  await expect(rowById(other, rowId).getByTestId("mode-chip")).toHaveAttribute(
    "data-bucket",
    after,
  );

  await page.keyboard.press("u");
  await expect(chip).toHaveAttribute("data-bucket", before);
  await expect(rowById(other, rowId).getByTestId("mode-chip")).toHaveAttribute(
    "data-bucket",
    before,
  );
  await other.close();
});
