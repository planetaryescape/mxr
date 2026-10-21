import { rmSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import type { ArchiveOperation } from "./helpers/archive";
import { cursorTo, mailRows, openList, rowById } from "./helpers/mail";
import { bridge, readE2EState } from "./helpers/state";
import { deliver, rejectMutationFor } from "./helpers/spool";

test.use({ viewport: { width: 1440, height: 900 } });

test("hover Archive acts at once on the reviewed selection, then Undo restores it", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const row = mailRows(page).first();
  const rowId = (await row.getAttribute("id"))!;
  const requests: string[] = [];
  const receipts: ArchiveOperation[] = [];
  // Archive is instant; a native dialog here would be the old browser confirmation.
  const dialogs: string[] = [];
  page.on("dialog", (dialog) => {
    dialogs.push(dialog.message());
    void dialog.dismiss();
  });
  page.on("request", (request) => {
    if (request.url().includes("/archive-operations/")) {
      requests.push(`${request.method()} ${new URL(request.url()).pathname}`);
    }
  });
  page.on("response", async (response) => {
    if (!response.url().includes("/archive-operations/")) return;
    const body: unknown = await response.json().catch(() => null);
    // SAFETY: this event handler filters to the Archive OpenAPI response envelope under test.
    const answer = body as { kind?: string; operation?: ArchiveOperation } | null;
    if (answer?.kind === "ArchiveOperation" && answer.operation) receipts.push(answer.operation);
  });

  const reviewResponse = page.waitForResponse((response) =>
    response.url().endsWith("/archive-operations/preview"),
  );
  const submitResponse = page.waitForResponse((response) =>
    response.url().endsWith("/archive-operations/submit"),
  );
  await row.hover();
  await expect(rowById(page, rowId)).toBeVisible();
  await row.getByTitle("Archive (e)").click();

  const review = await reviewResponse;
  expect(review.ok(), await review.text()).toBe(true);
  // SAFETY: this preview response is produced by the typed Archive OpenAPI route.
  const reviewed = (await review.json()) as { kind: string; operation: ArchiveOperation };
  expect(reviewed.kind).toBe("ArchiveOperation");
  expect(reviewed.operation.items).toHaveLength(1);

  const submission = await submitResponse;
  expect(submission.ok(), await submission.text()).toBe(true);
  // SAFETY: the request is emitted by the Archive client with the generated submit DTO.
  const submitBody = (await submission.request().postDataJSON()) as {
    identity: ArchiveOperation["identity"];
    review_id: string;
    mutation: { mutation: string; message_ids: string[] };
  };
  expect(submitBody.identity).toEqual(reviewed.operation.identity);
  expect(submitBody.review_id).toBe(reviewed.operation.review_id);
  expect(submitBody.mutation.message_ids).toEqual(
    reviewed.operation.items.map((item) => item.message_id),
  );
  expect(requests.indexOf("POST /api/v1/mail/archive-operations/preview")).toBeLessThan(
    requests.indexOf("POST /api/v1/mail/archive-operations/submit"),
  );
  await expect(page.getByText(/^Archived 1 messages?$/)).toBeVisible();
  expect(dialogs).toEqual([]);
  await expect(rowById(page, rowId)).toHaveCount(0);
  await expect
    .poll(() =>
      receipts.some((receipt) => receipt.items.every((item) => item.state === "completed")),
    )
    .toBe(true);
  const outcome = receipts.find((receipt) =>
    receipt.items.every((item) => item.state === "completed"),
  );
  expect(outcome?.undo_available).toBe(true);
  expect(outcome?.mutation_id).toBeTruthy();

  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(rowById(page, rowId)).toBeVisible();
});

test("a rejected Archive submit leaves the row in place and retains the review", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const rowId = (await mailRows(page).first().getAttribute("id"))!;
  let retainedReviewId: string | undefined;
  await page.route("**/api/v1/mail/archive-operations/submit", async (route) => {
    // SAFETY: this route only intercepts the Archive submit body from the client under test.
    const body = route.request().postDataJSON() as { review_id: string };
    retainedReviewId = body.review_id;
    await route.fulfill({
      status: 409,
      contentType: "application/json",
      json: { code: "conflict", error: "provider rejected the change" },
    });
  });

  const rejected = page.waitForResponse("**/api/v1/mail/archive-operations/submit");
  await page.keyboard.press("e");
  await rejected;
  await expect(page.getByText(/failed|rejected/i).first()).toBeVisible();
  await expect(rowById(page, rowId)).toBeVisible();
  expect(retainedReviewId).toBeTruthy();
  expect(
    await page.evaluate(
      (reviewId) =>
        Array.from({ length: localStorage.length }, (_, index) =>
          localStorage.getItem(localStorage.key(index) ?? ""),
        ).some((value) => value?.includes(reviewId ?? "")),
      retainedReviewId,
    ),
  ).toBe(true);
});

test("Archive from e can be undone with z", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = (await mailRows(page).first().getAttribute("id"))!;
  await page.keyboard.press("e");
  await expect(rowById(page, rowId)).toHaveCount(0);

  const undoResponse = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("z");
  expect((await undoResponse).ok()).toBe(true);
  await expect(rowById(page, rowId)).toBeVisible();
});

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
