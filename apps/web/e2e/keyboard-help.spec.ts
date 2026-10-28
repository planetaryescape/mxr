import { expect, test, type Page } from "@playwright/test";

import {
  mailList,
  mailRows,
  modKey,
  openList,
  pressSequence,
  reader,
  rowById,
} from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test("? opens keyboard help, not search", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("?");
  const help = page.getByRole("dialog", { name: "Keyboard" });
  await expect(help).toBeVisible();
  await expect(help.getByRole("textbox", { name: "Filter shortcuts" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Search mail" })).toHaveCount(0);
  // Exactly one help surface.
  await expect(page.getByRole("dialog")).toHaveCount(1);

  await page.keyboard.press("Escape");
  await expect(help).toHaveCount(0);
});

test("/ opens the search palette with the input focused", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("/");
  await expect(page.getByRole("textbox", { name: "Search mail" })).toBeFocused();
  await expect(page.getByRole("dialog", { name: "Keyboard" })).toHaveCount(0);
});

test("Mod+k opens the command palette and keys typed straight after land in it", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const urlBefore = page.url();

  // No wait between the chord and typing: letters must not leak into list
  // shortcuts (e would archive, s would star) while the palette mounts.
  await page.keyboard.press(`${await modKey(page)}+k`);
  await page.keyboard.type("settings", { delay: 0 });

  const palette = page.getByRole("dialog", { name: "Command palette" });
  await expect(palette).toBeVisible();
  await expect(palette.getByRole("combobox")).toHaveValue("settings");
  expect(page.url()).toBe(urlBefore);
  await expect(page.locator("[data-sonner-toast]")).toHaveCount(0);
});

test("g i from the reader goes to Inbox without starring the thread", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");
  const unstar = () => reader(page).getByRole("button", { name: "Unstar (s)", exact: true });
  const starredBefore = await unstar().count();

  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(mailList(page)).toBeVisible();
  await expect(page.getByText(/^(Starred|Unstarred) /)).toHaveCount(0);

  // Reopen: the star state is unchanged.
  await page.keyboard.press("Enter");
  await expect(reader(page)).toBeVisible();
  await expect(unstar()).toHaveCount(starredBefore);
});

test("g s from the reader goes to Starred and does not star", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");

  await pressSequence(page, "g", "s");
  await expect(page).toHaveURL(/\/m\/starred$/);
  await expect(page.getByText(/^(Starred|Unstarred) \d/)).toHaveCount(0);
});

/** Change keys.toml through the daemon, as the settings page does. */
async function setKeys(page: Page, change: Record<string, unknown>) {
  await bridge(page, "/api/v1/platform/keymap", { change, dry_run: false });
}

test.describe("keys.toml through the daemon", () => {
  // One daemon serves the whole run: put the defaults back for the next spec.
  test.afterEach(async ({ page }) => {
    await setKeys(page, { kind: "single_keys", on: true });
    await setKeys(page, { kind: "reset_action", id: "mail.archive" });
  });

  test("with single-key shortcuts off, e does nothing and the palette still archives", async ({
    page,
  }) => {
    await setKeys(page, { kind: "single_keys", on: false });
    let archiveRequests = 0;
    page.on("request", (request) => {
      if (/\/archive-operations\/submit|\/mutations\/archive/.test(request.url())) {
        archiveRequests += 1;
      }
    });
    await openList(page, "/m/inbox");
    const rowId = (await mailRows(page).first().getAttribute("id"))!;

    await page.keyboard.press("e");
    await page.keyboard.press("?");
    await expect(page.getByRole("dialog", { name: "Keyboard" })).toHaveCount(0);
    await expect(rowById(page, rowId)).toBeVisible();

    await page.keyboard.press(`${await modKey(page)}+k`);
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("combobox").fill("Archive");
    await palette
      .getByRole("option", { name: /^Archive( |$)/ })
      .first()
      .click();
    await expect(rowById(page, rowId)).toHaveCount(0);
    expect(archiveRequests).toBe(1);
  });

  test("a key saved in settings archives in the mail list", async ({ page }) => {
    await openApp(page, "/settings/keybindings");
    await page.getByRole("button", { name: "Change keys for Archive" }).first().click();
    await page.getByLabel(/Keys for Archive/).fill("%");
    await expect(page.getByText(/Archive will be % in the web app and the TUI/)).toBeVisible();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByText("Archive keys saved")).toBeVisible();

    await openList(page, "/m/inbox");
    const rowId = (await mailRows(page).first().getAttribute("id"))!;
    await page.keyboard.press("%");
    await expect(rowById(page, rowId)).toHaveCount(0);
  });
});
