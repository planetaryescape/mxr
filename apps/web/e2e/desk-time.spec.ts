import { expect, test, type Route } from "@playwright/test";

import { mailList } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

interface Row {
  thread_id: string;
  age_seconds: number;
  usual_seconds?: number | null;
  overdue?: boolean;
}

const HOUR = 3600;
const DAY = 24 * HOUR;

test("desk rows state time as it matters: age against the usual pace, late in the warning colour", async ({
  page,
}) => {
  // The daemon's desk, with its first owed and waiting rows given known
  // clocks: owed for 2 days where the person usually hears back in 4 hours
  // (late), and waiting 5 days with no known pace.
  await page.route("**/api/v1/mail/desk?**", async (route: Route) => {
    const upstream = await route.fetch();
    const desk = (await upstream.json()) as Record<"owed" | "waiting", { rows: Row[] }>;
    const owed = desk.owed.rows[0]!;
    Object.assign(owed, { age_seconds: 2 * DAY + HOUR, usual_seconds: 4 * HOUR, overdue: true });
    const waiting = desk.waiting.rows[0]!;
    Object.assign(waiting, { age_seconds: 5 * DAY, usual_seconds: null, overdue: false });
    await route.fulfill({ response: upstream, json: desk });
  });
  await openApp(page, "/desk");

  const owedTime = mailList(page).locator('[data-lane="owed"] time').first();
  await expect(owedTime).toHaveText("2d · usually 4h");
  await expect(owedTime).toHaveAttribute("data-late", "true");
  const waitingTime = mailList(page).locator('[data-lane="waiting"] time').first();
  await expect(waitingTime).toHaveText("5d");
  await expect(waitingTime).not.toHaveAttribute("data-late");

  // Late reads in the theme's warning colour; on time stays muted.
  const colours = await page.evaluate(() => {
    const warning = getComputedStyle(document.documentElement).getPropertyValue("--warning").trim();
    const probe = document.createElement("span");
    probe.style.color = warning;
    document.body.append(probe);
    const expected = getComputedStyle(probe).color;
    probe.remove();
    const [late, onTime] = ['[data-lane="owed"] time', '[data-lane="waiting"] time'].map(
      (selector) => getComputedStyle(document.querySelector(selector)!).color,
    );
    return { expected, late, onTime };
  });
  expect(colours.late).toBe(colours.expected);
  expect(colours.onTime).not.toBe(colours.expected);
});
