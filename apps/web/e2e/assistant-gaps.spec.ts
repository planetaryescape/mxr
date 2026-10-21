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

test("an email chip in a reply shows the glyph of the mode that holds it", async ({ page }) => {
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
});
