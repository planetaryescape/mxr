import { expect, test, type Page } from "@playwright/test";

import { mailList } from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Bundle {
  account_id: string;
  sender_email: string;
  message_count: number;
  pinned_count: number;
  messages: { message_id: string; pinned: boolean }[];
}

async function bridge<T>(page: Page, path: string, body?: unknown): Promise<T> {
  const state = readE2EState();
  const headers = { authorization: `Bearer ${state.token}` };
  const response = body
    ? await page.request.post(`${state.bridgeUrl}${path}`, { headers, data: body })
    : await page.request.get(`${state.bridgeUrl}${path}`, { headers });
  expect(response.ok(), `${path}: ${response.status()}`).toBeTruthy();
  return (await response.json()) as T;
}

function place(page: Page, slug: "reading") {
  return bridge<{ bundles: Bundle[]; total_messages: number }>(
    page,
    `/api/v1/mail/places/${slug}?messages_per_bundle=50`,
  );
}

function setKind(
  page: Page,
  bundle: Pick<Bundle, "account_id" | "sender_email">,
  kind: string | null,
) {
  return bridge(page, "/api/v1/mail/senders/kind", {
    account_id: bundle.account_id,
    sender_email: bundle.sender_email,
    kind,
  });
}

function sidebar(page: Page) {
  return page.getByRole("complementary", { name: "Mailboxes" });
}

test("moving a sender from Reading to People takes it to the desk, and u brings it back", async ({
  page,
}) => {
  const [bundle] = (await place(page, "reading")).bundles;
  expect(bundle).toBeTruthy();
  try {
    await openApp(page, "/reading");
    const issue = page.locator(
      `[data-testid='reading-item'][data-sender='${bundle!.sender_email}']`,
    );
    await expect(issue.first()).toBeVisible();
    await issue.first().click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("K");
    const menu = page.getByTestId("sender-kind-dialog");
    await expect(menu).toBeVisible();
    await menu.press("p");
    await expect(issue).toHaveCount(0);
    await expect(page.locator("[data-sonner-toast]").first()).toContainText("to People");

    // A person now: somewhere on the desk (the daemon's lanes decide
    // which), shown in full on the lane pages.
    const desk = await bridge<
      Record<string, { rows: { lane: string; counterparty_email: string }[] }>
    >(page, "/api/v1/mail/desk?lane_limit=500");
    const lane = ["owed", "due", "waiting", "people_new"]
      .flatMap((name) => desk[name]!.rows)
      .find((row) => row.counterparty_email === bundle!.sender_email)?.lane;
    expect(lane).toBeTruthy();
    // In-app navigation keeps the undo slot (a reload would drop it).
    await page.keyboard.press("g");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/messages$/);
    const row = page.locator(
      `[data-testid="messages-row"][data-row-id="person:${bundle!.sender_email.toLowerCase()}"]`,
    );
    // Decide only once Messages has rendered: isVisible() does not wait.
    await expect(page.getByTestId("messages-list")).toBeVisible();
    if (!(await row.isVisible())) await page.getByRole("button", { name: /^Quiet/ }).click();
    await expect(row).toBeVisible();

    // Undo restores the automatic kind: back in Reading.
    await page.keyboard.press("u");
    await expect(page.getByText(/is automatic again/).first()).toBeVisible();
    await page.keyboard.press("g");
    await page.keyboard.press("r");
    await expect(page).toHaveURL(/\/reading$/);
    await expect(issue.first()).toBeVisible();
  } finally {
    await setKind(page, bundle!, null);
  }
});

test("the desk's everything-else links open the places with week counts", async ({ page }) => {
  await openApp(page, "/desk");
  const elsewhere = page.getByRole("navigation", { name: "Everything else" });
  const reading = elsewhere.getByRole("link", { name: /Reading/ });
  await expect(reading).toContainText(/\d+ this week/);
  await reading.click();
  await expect(page).toHaveURL(/\/reading$/);
  await openApp(page, "/desk");
  const updates = elsewhere.getByRole("link", { name: /Updates/ });
  await expect(updates).toContainText(/\d+ this week/);
  await updates.click();
  await expect(page).toHaveURL(/\/updates$/);
  // Nothing in Updates is owed, so its rail entry carries no count.
  await expect(sidebar(page).getByRole("link", { name: /^Updates/ })).not.toContainText(/\d/);
});
