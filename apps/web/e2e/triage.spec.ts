import { expect, test } from "@playwright/test";

import type { ArchiveOperation } from "./helpers/archive";
import {
  expectCursorOn,
  mailList,
  openList,
  pressSequence,
  reader,
  renderedRowIds,
  rowById,
  stableRowName,
} from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

test("keyboard-only triage: open, archive-and-advance, find it in All Mail, undo", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const [first, second] = await renderedRowIds(page);
  expect(first && second).toBeTruthy();
  await expectCursorOn(page, first!);

  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  const openedUrl = page.url();
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");
  const archivedName = stableRowName((await rowById(page, first!).getAttribute("aria-label"))!);

  // Archive reviews first and submits exactly what the daemon reviewed.
  const reviewResponse = page.waitForResponse((response) =>
    response.url().endsWith("/archive-operations/preview"),
  );
  const submitResponse = page.waitForResponse((response) =>
    response.url().endsWith("/archive-operations/submit"),
  );
  await page.keyboard.press("e");
  const review = await reviewResponse;
  expect(review.ok(), await review.text()).toBe(true);
  // SAFETY: this preview response is produced by the typed Archive OpenAPI route.
  const reviewed = (await review.json()) as { operation: ArchiveOperation };
  const submission = await submitResponse;
  expect(submission.ok(), await submission.text()).toBe(true);
  // SAFETY: the request is emitted by the Archive client with the generated submit DTO.
  const submitted = submission.request().postDataJSON() as {
    identity: ArchiveOperation["identity"];
    review_id: string;
    mutation: { message_ids: string[] };
  };
  expect(submitted.identity).toEqual(reviewed.operation.identity);
  expect(submitted.review_id).toBe(reviewed.operation.review_id);
  expect(submitted.mutation.message_ids).toEqual(
    reviewed.operation.items.map((item) => item.message_id),
  );

  // The reader advances after the Archive operation completes.
  await expect(page).not.toHaveURL(openedUrl);
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  await expect(rowById(page, first!)).toHaveCount(0);
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible();

  await pressSequence(page, "g", "a");
  await expect(page).toHaveURL(/\/m\/archive$/);
  // All Mail titles a conversation by its latest message, which may be a "Re:".
  await expect(
    mailList(page)
      .getByRole("option", { name: conversationPattern(archivedName) })
      .first(),
  ).toBeVisible();

  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(rowById(page, first!)).toHaveCount(0);

  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
  await expect(rowById(page, first!)).toBeVisible();
});

function conversationPattern(stableName: string): RegExp {
  const parts = stableName.split(", ");
  const subject = parts.pop()!.replace(/^Re: /, "");
  const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`${escape(parts.join(", "))}, (Re: )?${escape(subject)},`);
}
