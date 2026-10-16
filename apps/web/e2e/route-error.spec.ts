import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("a bad thread id says so in plain words, offers the way to Inbox and never Try again", async ({
  page,
}) => {
  await openApp(page, "/m/inbox/not-a-real-thread-id-12345");

  await expect(page.getByRole("complementary", { name: "Mailboxes" })).toBeVisible();
  const error = page.getByTestId("page-error");
  await expect(
    error.getByRole("heading", { name: "Couldn't find this conversation" }),
  ).toBeVisible();
  await expect(error).toContainText("Your mail is untouched");
  await expect(error.getByRole("link", { name: "Go to Inbox" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Try again" })).toHaveCount(0);
  // The raw error is under Details, closed.
  const details = error.locator("details");
  await expect(details).not.toHaveAttribute("open", "");
  await details.getByText("Details").click();
  await expect(details).toContainText("invalid thread id");
  // The list beside it still works.
  await expect(page.getByTestId("mailbox-list")).toBeVisible();
});

test("a bad id's way out leads to the Inbox", async ({ page }) => {
  await openApp(page, "/m/inbox/not-a-real-thread-id-12345");
  await page.getByTestId("page-error").getByRole("link", { name: "Go to Inbox" }).click();
  await expect(page).toHaveURL(/\/m\/inbox$/);
});

test("a 500 on a list says what happened, what still works, and retries", async ({ page }) => {
  let failing = true;
  await page.route("**/api/v1/mail/mailbox?**", (route) =>
    failing
      ? route.fulfill({ status: 500, json: { error: "store exploded", code: "internal" } })
      : route.continue(),
  );
  await openApp(page, "/m/inbox");
  const error = page.getByTestId("page-error");
  await expect(error.getByRole("heading", { name: "Couldn't load Inbox" })).toBeVisible({
    timeout: 20_000,
  });
  await expect(error).toContainText("already on screen still works");
  await expect(error.getByText("store exploded")).toBeHidden();
  await error.locator("details").getByText("Details").click();
  await expect(error).toContainText("store exploded");
  failing = false;
  await error.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByTestId("mailbox-list").getByRole("option").first()).toBeVisible();
  await expect(page.getByTestId("page-error")).toHaveCount(0);
});
