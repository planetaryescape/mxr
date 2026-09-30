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

function place(page: Page, slug: "reading" | "paper-trail") {
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

test("Reading shows every issue already open, with a reason and no unread counts", async ({
  page,
}) => {
  const reading = await place(page, "reading");
  expect(reading.total_messages).toBeGreaterThan(0);
  await openApp(page, "/reading");
  const issues = page.getByTestId("reading-issue");
  await expect(issues.first()).toBeVisible();
  // Open means the body is there without a click.
  await expect(issues.first().getByTestId("issue-body")).toHaveAttribute("data-loaded", "true");
  await expect(issues.first().getByTestId("why-here")).toHaveText(/^Here because: .+\.$/);
  // Nothing in Reading counts: no unread badge in the sidebar or the page.
  await expect(sidebar(page).getByRole("link", { name: "Reading" })).not.toContainText(/\d/);
  await expect(page.getByRole("main")).not.toContainText(/unread/i);

  // j moves issue to issue.
  await expect(issues.first()).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("j");
  await expect(issues.nth(1)).toHaveAttribute("aria-current", "true");
});

test("moving a sender from Reading to People takes it to the desk, and u brings it back", async ({
  page,
}) => {
  const [bundle] = (await place(page, "reading")).bundles;
  expect(bundle).toBeTruthy();
  try {
    await openApp(page, "/reading");
    const issue = page.locator(
      `[data-testid='reading-issue'][data-sender='${bundle!.sender_email}']`,
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
    await page.keyboard.press("h");
    await expect(page).toHaveURL(/\/desk$/);
    const row = mailList(page).locator(`[title='${bundle!.sender_email}']`).first();
    if (!(await row.isVisible()))
      await page.locator(`a[href='/desk?lane=${lane}']`).first().click();
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

test("Paper trail: pin one, sweep the bundle without it, undo puts the rest back", async ({
  page,
}) => {
  // A sender with several messages in the inbox, moved to Paper trail for
  // this journey (the demo's automated senders have one each).
  const [anyBundle] = (await place(page, "reading")).bundles;
  const account = anyBundle!.account_id;
  const sender = { account_id: account, sender_email: "ari@fieldkit.example" };
  await setKind(page, sender, "paper_trail");
  try {
    const bundle = (await place(page, "paper-trail")).bundles.find(
      (item) => item.sender_email === sender.sender_email,
    );
    expect(bundle?.message_count ?? 0).toBeGreaterThan(1);
    const total = bundle!.message_count;

    await openApp(page, "/paper-trail");
    const row = page.locator(`[data-testid='place-bundle'][data-sender='${sender.sender_email}']`);
    await row.getByRole("button").click();
    await expect(row.getByRole("button")).toHaveAttribute("aria-expanded", "true");
    const messages = page.getByTestId("place-message");
    await messages.first().getByRole("button", { name: "Pin" }).click();
    await expect(messages.first()).toHaveAttribute("data-pinned", "true");

    // S sweeps the bundle under the cursor: the preview leaves the pin out.
    await page.keyboard.press("S");
    const dialog = page.getByTestId("sweep-dialog");
    await expect(dialog).toContainText(`Archive ${total - 1} message`);
    await expect(dialog).toContainText("1 pinned message stays");
    await dialog.getByRole("button", { name: /^Archive/ }).click();
    await expect(page.locator("[data-sonner-toast]").first()).toContainText(
      `Archived ${total - 1} message`,
    );
    await expect
      .poll(
        async () =>
          (await place(page, "paper-trail")).bundles.find(
            (b) => b.sender_email === sender.sender_email,
          )?.message_count,
        { timeout: 20_000 },
      )
      .toBe(1);

    await page.keyboard.press("u");
    await expect
      .poll(
        async () =>
          (await place(page, "paper-trail")).bundles.find(
            (b) => b.sender_email === sender.sender_email,
          )?.message_count,
        // Undo reverses each archived message; under a loaded suite that
        // takes longer than the default.
        { timeout: 20_000 },
      )
      .toBe(total);
  } finally {
    const bundle = (await place(page, "paper-trail")).bundles.find(
      (item) => item.sender_email === sender.sender_email,
    );
    const pinned = bundle?.messages.filter((m) => m.pinned).map((m) => m.message_id) ?? [];
    if (pinned.length > 0) {
      await bridge(page, "/api/v1/mail/messages/pin", { message_ids: pinned, pinned: false });
    }
    await setKind(page, sender, null);
  }
});

test("A previews the whole place, and Enter alone never sweeps it", async ({ page }) => {
  const preview = await bridge<{ preview: { count: number } }>(
    page,
    "/api/v1/mail/places/paper-trail/sweep",
    { dry_run: true },
  );
  await openApp(page, "/paper-trail");
  await expect(page.getByTestId("place-bundle").first()).toBeVisible();
  await page.keyboard.press("A");
  const dialog = page.getByTestId("sweep-dialog");
  await expect(dialog).toContainText(`Archive ${preview.preview.count} message`);
  await expect(dialog.getByRole("list", { name: "Senders" })).toBeVisible();
  // A slip from S to A, then Enter, archives nothing: the whole place
  // opens on Cancel, and its confirm names the scope.
  const confirm = dialog.getByRole("button", { name: /^Archive all \d[\d,]* from \d+ senders?$/ });
  await expect(confirm).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);

  // The real sweep needs a live preview token: a made-up one archives nothing.
  const state = readE2EState();
  const refused = await page.request.post(
    `${state.bridgeUrl}/api/v1/mail/places/paper-trail/sweep`,
    {
      headers: { authorization: `Bearer ${state.token}` },
      data: { dry_run: false, preview_token: "not-a-preview" },
    },
  );
  expect(refused.ok()).toBeFalsy();
  const after = await bridge<{ preview: { count: number } }>(
    page,
    "/api/v1/mail/places/paper-trail/sweep",
    { dry_run: true },
  );
  expect(after.preview.count).toBe(preview.preview.count);
});

test("A, Tab, Enter sweeps the whole place, and u puts it back", async ({ page }) => {
  const count = async () =>
    (
      await bridge<{ preview: { count: number } }>(page, "/api/v1/mail/places/paper-trail/sweep", {
        dry_run: true,
      })
    ).preview.count;
  const before = await count();
  expect(before).toBeGreaterThan(0);
  await openApp(page, "/paper-trail");
  await expect(page.getByTestId("place-bundle").first()).toBeVisible();
  await page.keyboard.press("A");
  const dialog = page.getByTestId("sweep-dialog");
  const confirm = dialog.getByRole("button", { name: /^Archive all / });
  await expect(confirm).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(confirm).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("[data-sonner-toast]").first()).toContainText("Archived");
  await expect.poll(count, { timeout: 20_000 }).toBe(0);
  await page.keyboard.press("u");
  await expect.poll(count, { timeout: 30_000 }).toBe(before);
});

test("the desk's everything-else links open the places with week counts", async ({ page }) => {
  await openApp(page, "/desk");
  const elsewhere = page.getByRole("navigation", { name: "Everything else" });
  const reading = elsewhere.getByRole("link", { name: /Reading/ });
  await expect(reading).toContainText(/\d+ this week/);
  await reading.click();
  await expect(page).toHaveURL(/\/reading$/);
  await openApp(page, "/desk");
  const paper = elsewhere.getByRole("link", { name: /Paper trail/ });
  await expect(paper).toContainText(/\d+ this week/);
  await paper.click();
  await expect(page).toHaveURL(/\/paper-trail$/);
  await expect(sidebar(page).getByRole("link", { name: "Paper trail" })).not.toContainText(/\d/);
});
