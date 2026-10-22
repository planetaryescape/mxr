import { expect, test } from "@playwright/test";

import { mockAssistant } from "./helpers/assistant";
import { openList } from "./helpers/mail";

/*
 * A1: the assistant matches its design chapter (docs/blueprint/29): the
 * daemon's irreversible line on a proof, and email chips that wear the glyph
 * of the mode holding the email. The panel's motion is in motion.spec.ts.
 */

test.use({ viewport: { width: 1440, height: 900 } });

test("a send waiting for approval shows the daemon's irreversible line above Apply", async ({
  page,
}) => {
  await mockAssistant(page, "sendProof");
  await openList(page, "/m/inbox");
  const proof = page.getByRole("region", { name: "Send a reply to Pat" });
  const warning = proof.getByText("Sends an email. Can't be undone.");
  await expect(warning).toBeVisible();
  const apply = proof.getByRole("button", { name: /^Apply/ });
  const [warningBox, applyBox] = await Promise.all([warning.boundingBox(), apply.boundingBox()]);
  expect(warningBox!.y).toBeLessThan(applyBox!.y);
});

test("an archive waiting for approval shows no irreversible line", async ({ page }) => {
  await mockAssistant(page, "proof");
  await openList(page, "/m/inbox");
  await expect(page.getByRole("region", { name: "Archive 14 emails" })).toBeVisible();
  await expect(page.getByText(/can't be undone/i)).toHaveCount(0);
});

test("the model mark is a menu: five choices, an unavailable one says why, a pick changes the mark", async ({
  page,
}) => {
  await mockAssistant(page, "empty");
  await openList(page, "/m/inbox");
  const mark = page.getByRole("button", { name: /^Model: Gemma 4/ });
  await expect(mark).toHaveText(/Gemma 4 · local/);

  await mark.click();
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitem")).toHaveCount(5);
  const luna = menu.getByRole("menuitem", { name: /GPT-6 Luna/ });
  await expect(luna).toBeDisabled();
  await expect(luna).toContainText("Add an OpenAI key: mxr llm key set openai");
  await expect(menu.getByRole("menuitem", { name: /Gemma 4/ })).toContainText("on this machine");
  await expect(menu.getByRole("menuitem", { name: /Claude Haiku 5.5/ })).toContainText("cloud");

  await menu.getByRole("menuitem", { name: /Claude Haiku 5.5/ }).click();
  await expect(menu).toBeHidden();
  await expect(page.getByRole("button", { name: /^Model: Claude Haiku 5.5/ })).toHaveText(
    /Claude Haiku 5.5 · cloud/,
  );
});

test("an email chip and its row in a reply show the glyph of the mode that holds it", async ({
  page,
}) => {
  await page.route("**/api/v1/mail/modes/membership", (route) =>
    route.fulfill({
      json: {
        kind: "ModeMembership",
        threads: [
          {
            thread_id: "t-ci-1",
            account_id: "acct",
            subject: "CI failed on main: build-linux",
            in_inbox: false,
            modes: [
              {
                mode: "reading",
                name: "Reading",
                key: "g r",
                reason: "",
                also_in: "In Reading",
              },
            ],
          },
        ],
      },
    }),
  );
  await mockAssistant(page, "finished");
  await openList(page, "/m/inbox");
  const chip = page.getByRole("link", { name: "CI failed on main: build-linux" }).first();
  await expect(chip.locator(".lucide-newspaper")).toBeVisible();
  await expect(chip.locator(".lucide-mail")).toHaveCount(0);

  // The reply's row for the same email is a compact card: who and a clip under
  // the subject, and the paperclip and mode glyph in the trail.
  const row = page
    .getByRole("list", { name: "The emails and people in this reply" })
    .getByRole("listitem")
    .first();
  await expect(row).toContainText("GitHub Actions · Run 4471 failed in build-linux");
  await expect(row.locator(".lucide-paperclip")).toBeVisible();
  await expect(row.locator(".lucide-newspaper")).toBeVisible();
  // An older row, with only a detail, has no marks.
  const old = page
    .getByRole("list", { name: "The emails and people in this reply" })
    .getByRole("listitem")
    .nth(1);
  await expect(old.locator(".lucide-paperclip, .lucide-mail")).toHaveCount(0);
});
