import { rmSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import type { ArchiveOperation } from "./helpers/archive";
import { cursorTo, mailRows, openList } from "./helpers/mail";
import { bridge, readE2EState } from "./helpers/state";
import { deliver, rejectMutationFor } from "./helpers/spool";

test.use({ viewport: { width: 1440, height: 900 } });

test("a provider rejection in a multi-message Archive keeps Undo for the completed item", async ({
  page,
}) => {
  const rejectedName = `archive-reject-${Date.now()}`;
  const completedName = `archive-complete-${Date.now()}`;
  const rejectedFileName = deliver({
    from: "archive-reject@example.test",
    subject: rejectedName,
    body: "This rejected Archive fixture has readable text for the reader.",
  });
  const completedFileName = deliver({
    from: "archive-complete@example.test",
    subject: completedName,
    body: "This completed Archive fixture has readable text for the reader.",
  });
  const rejectedProviderId = `spool-${rejectedFileName}`;
  const completedProviderId = `spool-${completedFileName}`;
  const removeRejection = rejectMutationFor(rejectedProviderId);
  const cleanup = () => {
    removeRejection();
    rmSync(join(readE2EState().runtimeDir, "spool", `${rejectedFileName}.msg`), {
      force: true,
    });
    rmSync(join(readE2EState().runtimeDir, "spool", `${completedFileName}.msg`), {
      force: true,
    });
  };
  try {
    await bridge(page, "/api/v1/mail/sync", {});
    await openList(page, "/m/inbox");
    await expect(mailRows(page).filter({ hasText: rejectedName })).toBeVisible();
    await expect(mailRows(page).filter({ hasText: completedName })).toBeVisible();
    await page.getByTestId("mailbox-list").focus();
    await page.keyboard.press("Home");
    await cursorTo(page, (label) => label.includes(rejectedName));
    await page.keyboard.press("x");
    await page.keyboard.press("Home");
    await cursorTo(page, (label) => label.includes(completedName));
    await page.keyboard.press("x");

    await page.keyboard.press("e");
    await expect(page.getByText(/1 confirmed complete; .*not_executed:/).first()).toBeVisible();

    let receipt: ArchiveOperation | undefined;
    await expect
      .poll(async () => {
        const retained = await page.evaluate(() =>
          Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index))
            .filter((key): key is string => key?.startsWith("mxr.archive.operation.") ?? false)
            .flatMap((key) => {
              const value = localStorage.getItem(key);
              if (!value) return [];
              // SAFETY: the client retained this receipt from its validated Archive API response.
              return [JSON.parse(value) as ArchiveOperation];
            }),
        );
        receipt = retained.find((operation) =>
          operation.items.some((item) => item.provider_id === rejectedProviderId),
        );
        return receipt?.items.length;
      })
      .toBe(2);
    expect(receipt?.items.map((item) => [item.provider_id, item.state])).toEqual(
      expect.arrayContaining([
        [rejectedProviderId, "not_executed"],
        [completedProviderId, "completed"],
      ]),
    );
    expect(receipt?.undo_available).toBe(true);

    await page.keyboard.press("u");
    await expect(page.getByText(/^Undone$/).first()).toBeVisible();
    await expect
      .poll(async () => {
        const mailbox = await bridge<{ mailbox: { groups: { rows: { subject: string }[] }[] } }>(
          page,
          "/api/v1/mail/mailbox?lens_kind=inbox&view=threads&limit=200&offset=0",
        );
        return mailbox.mailbox.groups.flatMap((group) => group.rows).map((row) => row.subject);
      })
      .toEqual(expect.arrayContaining([rejectedName, completedName]));
  } finally {
    cleanup();
  }
});
