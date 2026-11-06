import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test.beforeEach(async ({ page }) => {
  // These threads exist only in the fixtures, including their local mode metadata.
  await page.route(/\/api\/v1\/mail\/modes\/membership\?thread_id=thread-html$/, async (route) => {
    await route.fulfill({ contentType: "application/json", json: { threads: [] } });
  });
});

test("HTML mail renders in a sandbox without scripts, trackers stripped and remote images held back", async ({
  page,
}) => {
  const row = {
    id: "msg-html",
    kind: "message",
    thread_id: "thread-html",
    provider_id: "provider-html",
    sender: "Security Smoke",
    sender_detail: "Security@Example.COM",
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
            text_html: `<script>window.parent.PWNED=1</script><p>visible body</p><p id="white-panel" style="background-color:#fff;color:#777;font-size:12px">fine print <a href="https://example.com" style="text-decoration:none;color:#777">visible link</a></p><div style="color:#111"><div id="inherited-callout" style="background:#e8f0fe">Important condition</div></div><div id="modern-white" style="background-color:rgb(255 255 255)">Modern white</div><img src="https://cdn.example.com/newsletter.png" width="640" height="360" alt="newsletter image"><img src="https://track.customer.io/open.png" width="640" height="360" alt="tracking pixel">`,
            reader_text: "reader fallback",
            attachments: [],
          },
        ],
      },
    });
  });

  await openApp(page, "/m/inbox");
  await page.getByRole("option", { name: /sandbox smoke/ }).click();
  const reader = page.getByRole("article", { name: "Conversation: sandbox smoke" });
  await expect(reader).toBeVisible();

  const frame = reader.locator("iframe[sandbox]").first();
  await expect(frame).toBeVisible();
  await expect(frame).toHaveAttribute("sandbox", /allow-popups/);
  await expect(frame).not.toHaveAttribute("sandbox", /allow-scripts/);
  expect(await page.evaluate(() => (window as { PWNED?: number }).PWNED)).toBeUndefined();
  const body = frame.contentFrame();
  await expect(body.getByText("visible body")).toBeVisible();
  // Keys still reach the app after a click in the message. WebKit runs no
  // listeners inside the script-less frame, so the page takes focus back.
  await body.getByText("visible body").click();
  await page.keyboard.press("?");
  const help = page.getByRole("dialog", { name: "Help and keyboard shortcuts" });
  await expect(help).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(help).toBeHidden();
  // Tracking pixels are stripped; other remote images wait for consent.
  await expect(body.getByAltText("tracking pixel")).toHaveCount(0);
  // A blocked image is a small inline note, not an empty box its size.
  await expect(body.getByAltText("newsletter image")).toHaveCount(0);
  const note = body.locator(".mxr-image-blocked");
  await expect(note).toHaveText("Image blocked");
  await expect(note).toHaveAttribute("title", "newsletter image");
  expect((await note.boundingBox())?.height ?? 0).toBeLessThan(30);
  await expect(reader.getByTestId("privacy-line")).toContainText(
    "Blocked 1 tracker and 1 remote image from Customer.io and example.com.",
  );
  await reader.getByRole("button", { name: /^Show images/ }).click();
  await expect(
    reader.locator("iframe[sandbox]").first().contentFrame().getByAltText("newsletter image"),
  ).toHaveAttribute("src", "https://cdn.example.com/newsletter.png");
});
