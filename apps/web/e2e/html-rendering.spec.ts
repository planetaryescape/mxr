import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("HTML body renders inside a sandboxed iframe", async ({ page }) => {
  const row = {
    id: "msg-html",
    kind: "message",
    thread_id: "thread-html",
    provider_id: "provider-html",
    sender: "Security Smoke",
    sender_detail: "security@example.com",
    subject: "sandbox smoke",
    snippet: "visible body",
    date: "2026-05-11T00:00:00Z",
    date_label: "now",
    date_full: "May 11, 2026, 12:00 AM",
    date_relative: "2 hours ago",
    to: [{ name: "Planetary Escape", email: "planetary@example.com" }],
    labels: [{ id: "label-inbox", name: "Inbox", kind: "system" }],
    unread: false,
    starred: false,
    has_attachments: false,
  };

  await page.route("**/api/v1/mail/mailbox?*", async (route) => {
    await route.fulfill({
      contentType: "application/json",
      json: {
        mailbox: {
          lensLabel: "Inbox",
          view: "threads",
          counts: { unread: 0, total: 1 },
          groups: [{ id: "today", label: "Today", rows: [row] }],
        },
      },
    });
  });

  await page.route("**/api/v1/mail/threads/thread-html", async (route) => {
    await route.fulfill({
      contentType: "application/json",
      json: {
        thread: {
          account_id: "account-fake",
          id: "thread-html",
          latest_date: "2026-05-11T00:00:00Z",
          message_count: 1,
          participants: [{ name: "Security Smoke", email: "security@example.com" }],
          snippet: "visible body",
          subject: "sandbox smoke",
          unread_count: 0,
        },
        messages: [row],
        bodies: [
          {
            message_id: "msg-html",
            text_plain: "plain fallback",
            text_html: `<script>window.parent.PWNED=1</script><p>visible body</p><p id="white-panel" style="background-color:#fff;color:#777;font-size:12px">fine print <a href="https://example.com" style="text-decoration:none;color:#777">visible link</a></p><img src="https://cdn.example.com/newsletter.png" width="640" height="360" alt="newsletter image"><img src="https://track.customer.io/open.png" width="640" height="360" alt="tracking pixel">`,
            reader_text: "reader fallback",
            attachments: [],
          },
        ],
      },
    });
  });

  await openApp(page, "/m/inbox");
  await page.getByRole("heading", { name: "sandbox smoke" }).click();

  const frame = page.locator("iframe[sandbox]").first();
  await expect(frame).toBeVisible();
  await expect(frame).toHaveAttribute("sandbox", /allow-popups/);
  await expect(frame).not.toHaveAttribute("sandbox", /allow-scripts/);
  expect(await page.evaluate(() => (window as { PWNED?: number }).PWNED)).toBeUndefined();
  await expect(frame.contentFrame().getByText("visible body")).toBeVisible();
  await expect(frame.contentFrame().getByText("fine print")).toHaveCSS("color", "rgb(119, 119, 119)");
  await expect(frame.contentFrame().getByText("fine print")).toHaveCSS("font-size", "12px");
  await expect(frame.contentFrame().locator("#white-panel")).not.toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  await expect(frame.contentFrame().getByRole("link", { name: "visible link" })).toHaveCSS(
    "text-decoration-line",
    "underline",
  );
  await expect(frame.contentFrame().getByAltText("newsletter image")).toHaveAttribute(
    "src",
    "https://cdn.example.com/newsletter.png",
  );
  await expect(frame.contentFrame().getByAltText("tracking pixel")).toHaveCount(0);
  await expect(frame.contentFrame().locator("body")).toHaveCSS("background-color", "rgb(17, 17, 15)");

  const heights = await page.getByTestId("thread-scroll").evaluate((scrollNode) => {
    const messageNode = scrollNode.querySelector('[data-testid="thread-message"]');
    const frameNode = scrollNode.querySelector("iframe");
    if (!messageNode) throw new Error("thread message not found");
    if (!frameNode) throw new Error("message iframe not found");
    return {
      scroll: scrollNode.getBoundingClientRect().height,
      message: messageNode.getBoundingClientRect().height,
      frame: frameNode.getBoundingClientRect().height,
    };
  });
  expect(heights.message).toBeGreaterThan(heights.scroll * 0.75);
  expect(heights.frame).toBeGreaterThan(150);

  await expect(page.getByText("Inbox").last()).toBeVisible();
  await expect(page.getByText("to Planetary Escape <planetary@example.com>")).toBeVisible();
  await expect(page.getByText("now").last()).toContainText("2 hours ago");
  const reader = page.getByRole("article", { name: "Thread reader" });
  await expect(reader.getByRole("button", { name: /star/i })).toBeVisible();
  await expect(reader.getByRole("button", { name: /^archive$/i })).toBeVisible();
  await expect(reader.getByRole("button", { name: /^spam$/i })).toBeVisible();
  await expect(reader.getByRole("button", { name: /^mark unread$/i })).toBeVisible();
  await expect(reader.getByRole("button", { name: /^reply$/i })).toBeVisible();
  await expect(reader.getByRole("button", { name: /^reply all$/i })).toBeVisible();
});

test("message attachments can be opened and downloaded", async ({ page }) => {
  const row = {
    id: "msg-attachment",
    kind: "message",
    thread_id: "thread-attachment",
    provider_id: "provider-attachment",
    sender: "Attachment Smoke",
    sender_detail: "attachment@example.com",
    subject: "attachment smoke",
    snippet: "see attached",
    date: "2026-05-11T00:00:00Z",
    date_label: "now",
    date_full: "May 11, 2026, 12:00 AM",
    date_relative: "2 hours ago",
    to: [{ name: "Planetary Escape", email: "planetary@example.com" }],
    unread: false,
    starred: false,
    has_attachments: true,
  };
  let openBody: unknown;
  let downloadBody: unknown;

  await page.route("**/api/v1/mail/mailbox?*", async (route) => {
    await route.fulfill({
      contentType: "application/json",
      json: {
        mailbox: {
          lensLabel: "Inbox",
          view: "threads",
          counts: { unread: 0, total: 1 },
          groups: [{ id: "today", label: "Today", rows: [row] }],
        },
      },
    });
  });

  await page.route("**/api/v1/mail/threads/thread-attachment", async (route) => {
    await route.fulfill({
      contentType: "application/json",
      json: {
        thread: {
          account_id: "account-fake",
          id: "thread-attachment",
          latest_date: "2026-05-11T00:00:00Z",
          message_count: 1,
          participants: [{ name: "Attachment Smoke", email: "attachment@example.com" }],
          snippet: "see attached",
          subject: "attachment smoke",
          unread_count: 0,
        },
        messages: [row],
        bodies: [
          {
            message_id: "msg-attachment",
            text_plain: "plain fallback",
            text_html: `<p>see attached</p>`,
            reader_text: "reader fallback",
            attachments: [
              {
                id: "att-report",
                message_id: "msg-attachment",
                filename: "report.pdf",
                mime_type: "application/pdf",
                size_bytes: 4096,
              },
            ],
          },
        ],
      },
    });
  });

  await page.route("**/api/v1/mail/attachments/open", async (route) => {
    openBody = route.request().postDataJSON();
    await route.fulfill({ contentType: "application/json", json: { file: "/tmp/report.pdf" } });
  });
  await page.route("**/api/v1/mail/attachments/download", async (route) => {
    downloadBody = route.request().postDataJSON();
    await route.fulfill({ contentType: "application/json", json: { file: "/tmp/report.pdf" } });
  });

  await openApp(page, "/m/inbox");
  await page.getByRole("heading", { name: "attachment smoke" }).click();

  await page.getByRole("button", { name: "Open report.pdf" }).click();
  await expect.poll(() => openBody).toEqual({ message_id: "msg-attachment", attachment_id: "att-report" });

  await page.getByRole("button", { name: "Download report.pdf" }).click();
  await expect
    .poll(() => downloadBody)
    .toEqual({ message_id: "msg-attachment", attachment_id: "att-report" });

  const layout = await page.getByTestId("thread-scroll").evaluate((scrollNode) => {
    const frameNode = scrollNode.querySelector("iframe");
    const attachmentNode = scrollNode.querySelector('[data-testid="attachment-actions"]');
    if (!frameNode) throw new Error("message iframe not found");
    if (!attachmentNode) throw new Error("attachment actions not found");

    const scrollRect = scrollNode.getBoundingClientRect();
    const frameRect = frameNode.getBoundingClientRect();
    const attachmentRect = attachmentNode.getBoundingClientRect();
    return {
      scrollHeight: scrollRect.height,
      frameHeight: frameRect.height,
      gapBeforeAttachments: attachmentRect.top - frameRect.bottom,
    };
  });

  expect(layout.frameHeight).toBeGreaterThan(150);
  expect(layout.gapBeforeAttachments).toBeLessThan(32);
});
