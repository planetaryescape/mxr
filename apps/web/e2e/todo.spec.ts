import { expect, test, type Page } from "@playwright/test";

import { mailRows, openList } from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface TodoAnswer {
  id: string;
  title: string;
  kind: string;
  when_label: string;
  state: string;
  catchup?: string | null;
  source_message_id?: string | null;
}
interface RunwayAnswer {
  runway: {
    now: TodoAnswer[];
    coming_up: { label: string; todos: TodoAnswer[] }[];
    whenever: TodoAnswer[];
    later: TodoAnswer[];
  };
}
interface CatchupAnswer {
  catchup: { todos: TodoAnswer[]; already_over_line?: string | null };
}

/** The demo's first run is done and the council tax bill is on the runway. */
async function waitForRunway(page: Page): Promise<RunwayAnswer["runway"]> {
  let runway: RunwayAnswer["runway"] | null = null;
  await expect
    .poll(
      async () => {
        runway = (await bridge<RunwayAnswer>(page, "/api/v1/mail/todos")).runway;
        return runway.coming_up.flatMap((week) => week.todos).map((todo) => todo.title);
      },
      { timeout: 60_000 },
    )
    .toContain("Pay council tax");
  return runway!;
}

function row(page: Page, title: string) {
  return page.getByTestId("todo-row").filter({ hasText: title });
}

test("the bill waits in Coming up with its act-by date, and rows are instructions", async ({
  page,
}) => {
  await waitForRunway(page);
  await openApp(page, "/todo");
  const week = page.getByTestId("coming-week").filter({ hasText: "Pay council tax" });
  await expect(week).toBeVisible();
  const bill = row(page, "Pay council tax");
  await expect(bill.getByTestId("todo-when")).toContainText("act by");
  await expect(bill.getByTestId("todo-amount")).toHaveText("£142.00");
  await expect(bill).toHaveAttribute("data-band", "coming");
  // A row's title is what to do, never its email's subject.
  const now = row(page, "Fix payment for Spotify");
  await expect(now).toBeVisible();
  const titles = await page.getByTestId("todo-title").allTextContents();
  expect(titles).not.toContain("We can't process your payment");
  expect(titles).not.toContain("Your council tax bill");
  await expect(now.getByTestId("todo-action")).toHaveText(/^Open email to/);
});

test("Enter opens the email in mxr with the link marked, and never opens the link", async ({
  page,
  context,
}) => {
  await waitForRunway(page);
  await openApp(page, "/todo");
  await expect(row(page, "Fix payment for Spotify")).toHaveAttribute("aria-current", "true");
  const popups: string[] = [];
  context.on("page", (opened) => popups.push(opened.url()));
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/todo\/[^/?]+/);
  await expect(page.getByTestId("todo-link-note")).toContainText("spotify.com");
  await expect(page.locator("[data-todo-link]").first()).toBeVisible();
  expect(popups).toEqual([]);
});

test("e ticks off, and u brings it back", async ({ page }) => {
  await waitForRunway(page);
  await openApp(page, "/todo");
  const spotify = row(page, "Fix payment for Spotify");
  await expect(spotify).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("e");
  await expect(page.getByText("Ticked off: Fix payment for Spotify")).toBeVisible();
  await expect(spotify).toHaveCount(0);
  await expect(page.getByTestId("done-toggle")).toContainText("Done this week");
  await page.keyboard.press("u");
  await expect(page.getByText("Undone")).toBeVisible();
  await expect(row(page, "Fix payment for Spotify")).toHaveAttribute("data-band", "now");
  const runway = (await bridge<RunwayAnswer>(page, "/api/v1/mail/todos")).runway;
  expect(runway.now.map((todo) => todo.title)).toContain("Fix payment for Spotify");
});

test("an invite for an event that already happened never shows", async ({ page }) => {
  const runway = await waitForRunway(page);
  const titles = [
    ...runway.now,
    ...runway.coming_up.flatMap((week) => week.todos),
    ...runway.later,
    ...runway.whenever,
  ].map((todo) => todo.title);
  expect(titles.some((title) => /Sam/.test(title))).toBe(false);
  await openApp(page, "/todo");
  await expect(row(page, "Fix payment for Spotify")).toBeVisible();
  await expect(page.getByText(/leaving drinks/i)).toHaveCount(0);
  await openApp(page, "/todo?view=catchup");
  await expect(page.getByText("Already over, so not shown: 1 past invite.")).toBeVisible();
  await expect(page.getByText(/leaving drinks/i)).toHaveCount(0);
});

test("letting go of the whole catch-up changes exactly what its preview listed", async ({
  page,
}) => {
  await waitForRunway(page);
  const before = (await bridge<CatchupAnswer>(page, "/api/v1/mail/todos/catchup")).catchup;
  expect(before.todos.length).toBeGreaterThan(0);
  await openApp(page, "/todo?view=catchup");
  await expect(page.getByTestId("catchup-title")).toContainText("Catch up:");
  await page.keyboard.press("A");
  const dialog = page.getByTestId("let-go-all-dialog");
  const previewRows = dialog.getByTestId("let-go-preview-row");
  await expect(previewRows).toHaveCount(before.todos.length);
  const previewed = (await previewRows.evaluateAll((nodes) =>
    nodes.map((node) => node.getAttribute("data-id")),
  )) as string[];
  await dialog.getByRole("button", { name: /^Let go of/ }).click();
  await expect(page.getByText(/^Let go of \d+ to-dos?$/)).toBeVisible();

  const expired = (await bridge<{ todos: TodoAnswer[] }>(page, "/api/v1/mail/todos/in/expired"))
    .todos;
  const letGo = expired.filter((todo) => todo.catchup === "let_go").map((todo) => todo.id);
  expect(letGo.toSorted()).toEqual(previewed.toSorted());
  const after = (await bridge<CatchupAnswer>(page, "/api/v1/mail/todos/catchup")).catchup;
  expect(after.todos).toEqual([]);

  // Undo puts every one back in the batch, undecided.
  await page.keyboard.press("u");
  await expect(page.getByText("Undone")).toBeVisible();
  await expect
    .poll(async () =>
      (await bridge<CatchupAnswer>(page, "/api/v1/mail/todos/catchup")).catchup.todos
        .map((todo) => todo.id)
        .toSorted(),
    )
    .toEqual(previewed.toSorted());
});

test("t on a conversation makes a to-do in your words", async ({ page }) => {
  await waitForRunway(page);
  await openList(page, "/m/inbox");
  await mailRows(page).first().click();
  await page.keyboard.press("t");
  const dialog = page.getByRole("dialog", { name: "Make a to-do" });
  await expect(dialog).toBeVisible();
  const title = dialog.getByLabel("What to do");
  await title.fill("Send Maya the venue options");
  await dialog.getByRole("button", { name: "Add to To do" }).click();
  await expect(page.getByText("Added to To do: Send Maya the venue options")).toBeVisible();
  const runway = (await bridge<RunwayAnswer>(page, "/api/v1/mail/todos")).runway;
  const all = [...runway.now, ...runway.whenever, ...runway.coming_up.flatMap((w) => w.todos)];
  expect(all.map((todo) => todo.title)).toContain("Send Maya the venue options");
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the bands stack and nothing scrolls sideways", async ({ page }) => {
    await waitForRunway(page);
    for (const path of ["/todo", "/todo?view=catchup"]) {
      await openApp(page, path);
      await expect(page.getByTestId("todo-row").first()).toBeVisible();
      await page.waitForLoadState("networkidle");
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow, path).toBe(0);
    }
    await openApp(page, "/todo");
    // The button spans the card, so the thumb hits the action, not the row.
    const button = row(page, "Fix payment for Spotify").getByTestId("todo-action");
    const box = await button.boundingBox();
    expect(box?.width ?? 0).toBeGreaterThan(250);
  });
});
