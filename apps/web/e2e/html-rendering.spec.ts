import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

function contrastRatio(foreground: string, background: string): number {
  const luminance = (color: string) => {
    const channels = color
      .match(/\d+(?:\.\d+)?/g)
      ?.slice(0, 3)
      .map(Number);
    if (!channels || channels.length < 3) throw new Error(`could not read color: ${color}`);
    const linear = channels.map((value) => {
      const channel = value / 255;
      return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    });
    return linear[0]! * 0.2126 + linear[1]! * 0.7152 + linear[2]! * 0.0722;
  };
  const first = luminance(foreground);
  const second = luminance(background);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

// The white-panel check below is about the dark reader; on a light theme a
// white panel is expected.
test.use({ colorScheme: "dark" });

test("HTML body renders inside a sandboxed iframe", async ({ page }) => {
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
  await expect(body.getByText("fine print")).toHaveCSS("color", "rgb(108, 108, 108)");
  await expect(body.getByText("fine print")).toHaveCSS("font-size", "12px");
  // A white panel from a light-theme email is not pasted onto the dark reader.
  await expect(body.locator("#white-panel")).not.toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  const paperContrast = await frame.evaluate((element) => {
    if (!(element instanceof HTMLIFrameElement)) throw new Error("message frame not found");
    const doc = element.contentDocument;
    const view = doc?.defaultView;
    const text = doc?.querySelector("#white-panel");
    if (!doc || !view || !text) throw new Error("paper contrast fixture not found");
    const foreground = view.getComputedStyle(text).color;
    const background = view.getComputedStyle(text).backgroundColor;
    const channel = (value: string) =>
      value
        .match(/\d+(?:\.\d+)?/g)
        ?.slice(0, 3)
        .map(Number);
    const fg = channel(foreground);
    const bg = channel(background);
    if (!fg || !bg || fg.length < 3 || bg.length < 3)
      throw new Error(`could not read paper colors: ${foreground} / ${background}`);
    const luminance = ([red, green, blue]: number[]) => {
      const linear = [red, green, blue].map((value) => {
        const channel = value ?? 0;
        const c = channel / 255;
        return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      });
      return linear[0]! * 0.2126 + linear[1]! * 0.7152 + linear[2]! * 0.0722;
    };
    const first = luminance(fg);
    const second = luminance(bg);
    return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
  });
  expect(paperContrast).toBeGreaterThanOrEqual(4.5);
  await expect(body.locator("#modern-white")).toHaveCSS("background-color", "rgb(241, 238, 233)");
  await expect(body.getByRole("link", { name: "visible link" })).toHaveCSS(
    "text-decoration-line",
    "underline",
  );
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

  await reader.getByRole("button", { name: "Email appearance: auto" }).click();
  await page.getByRole("menuitemradio", { name: "Dark for this message" }).click();
  const callout = frame.contentFrame().locator("#inherited-callout");
  await expect(callout).toBeVisible();
  await expect(frame.contentFrame().locator("body")).toHaveCSS(
    "background-color",
    "rgb(13, 22, 31)",
  );
  const contrast = await frame.evaluate((element) => {
    if (!(element instanceof HTMLIFrameElement)) throw new Error("message frame not found");
    const doc = element.contentDocument;
    const view = doc?.defaultView;
    if (!doc || !view) throw new Error("email frame is not readable");
    const calloutNode = doc.querySelector("#inherited-callout");
    if (!calloutNode) throw new Error("callout not found");
    const foreground = view.getComputedStyle(calloutNode).color;
    let backgroundNode: Element | null = calloutNode;
    let background = "transparent";
    while (backgroundNode) {
      const candidate = view.getComputedStyle(backgroundNode).backgroundColor;
      if (!candidate.endsWith(", 0)")) {
        background = candidate;
        break;
      }
      backgroundNode = backgroundNode.parentElement;
    }
    const channels = (color: string): [number, number, number] | null => {
      const values = color
        .match(/\d+(?:\.\d+)?/g)
        ?.slice(0, 3)
        .map(Number);
      if (!values || values.length < 3 || values.some((value) => !Number.isFinite(value))) {
        return null;
      }
      return [values[0] ?? Number.NaN, values[1] ?? Number.NaN, values[2] ?? Number.NaN];
    };
    const fg = channels(foreground);
    const bg = channels(background);
    if (!fg || !bg)
      throw new Error(`could not read computed colors: ${foreground} / ${background}`);
    const luminance = ([red, green, blue]: [number, number, number]) => {
      const linear = [red, green, blue].map((value) => {
        const channel = value / 255;
        return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
      });
      return linear[0]! * 0.2126 + linear[1]! * 0.7152 + linear[2]! * 0.0722;
    };
    const foregroundLuminance = luminance(fg);
    const backgroundLuminance = luminance(bg);
    const light = Math.max(foregroundLuminance, backgroundLuminance);
    const dark = Math.min(foregroundLuminance, backgroundLuminance);
    return (light + 0.05) / (dark + 0.05);
  });
  expect(contrast).toBeGreaterThanOrEqual(4.5);
  await test.info().attach("explicit-dark-inherited-callout", {
    body: await frame.screenshot(),
    contentType: "image/png",
  });

  await reader.getByRole("button", { name: "Email appearance: dark" }).click();
  await page.getByRole("menuitem", { name: "Always show Original" }).click();
  const storedSenders = await page.evaluate(() => {
    return localStorage.getItem("mxr.uiPrefs");
  });
  expect(storedSenders).toContain(
    '"emailHtmlSenderOverrides":[{"sender":"security@example.com","theme":"original"}]',
  );

  await page.reload();
  await page.getByRole("option", { name: /sandbox smoke/ }).click();
  const reloadedReader = page.getByRole("article", { name: "Conversation: sandbox smoke" });
  const reloadedFrame = reloadedReader.locator("iframe[sandbox]").first();
  await expect(
    reloadedReader.getByRole("button", { name: "Email appearance: original" }),
  ).toBeVisible();
  await expect(reloadedFrame.contentFrame().locator("body")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  await reloadedReader.getByRole("button", { name: "Email appearance: original" }).click();
  await page.getByRole("menuitemradio", { name: "Auto for this message" }).click();
  await expect(reloadedFrame.contentFrame().locator("body")).toHaveCSS(
    "background-color",
    "rgb(241, 238, 233)",
  );
  await reloadedReader.getByRole("button", { name: "Email appearance: auto" }).click();
  await page.getByRole("menuitemradio", { name: "Use sender/default" }).click();
  await expect(reloadedFrame.contentFrame().locator("body")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  await reloadedReader.getByRole("button", { name: "Email appearance: original" }).click();
  await page.getByRole("menuitem", { name: "Reset sender preference" }).click();
  await expect(
    reloadedReader.getByRole("button", { name: "Email appearance: auto" }),
  ).toBeVisible();
  const reloadedBody = reloadedFrame.contentFrame().locator("body");
  await expect(reloadedBody).toHaveCSS("background-color", "rgb(241, 238, 233)");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(reloadedBody).toHaveCSS("background-color", "rgb(255, 255, 255)");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(reloadedBody).toHaveCSS("background-color", "rgb(241, 238, 233)");

  await page.evaluate(() => {
    const saved = JSON.parse(localStorage.getItem("mxr.uiPrefs") ?? "{}");
    saved.state.theme = "light";
    localStorage.setItem("mxr.uiPrefs", JSON.stringify(saved));
  });
  await page.reload();
  await page.getByRole("option", { name: /sandbox smoke/ }).click();
  const lightReader = page.getByRole("article", { name: "Conversation: sandbox smoke" });
  const lightFrame = lightReader.locator("iframe[sandbox]").first();
  await lightReader.getByRole("button", { name: "Email appearance: auto" }).click();
  await page.getByRole("menuitemradio", { name: "Dark for this message" }).click();
  await expect(lightFrame.contentFrame().locator("body")).toHaveCSS(
    "background-color",
    "rgb(13, 22, 31)",
  );
  const colors = await lightFrame.evaluate((element) => {
    if (!(element instanceof HTMLIFrameElement)) throw new Error("message frame not found");
    const doc = element.contentDocument;
    const view = doc?.defaultView;
    const text = doc?.querySelector("#inherited-callout");
    if (!doc || !view || !text) throw new Error("dark contrast fixture not found");
    return [
      view.getComputedStyle(text).color,
      view.getComputedStyle(doc.body).backgroundColor,
    ] as const;
  });
  expect(contrastRatio(colors[0], colors[1])).toBeGreaterThanOrEqual(4.5);
  await page.setViewportSize({ width: 390, height: 844 });
  const appearance = lightReader.getByRole("button", { name: "Email appearance: dark" });
  const appearanceBox = await appearance.boundingBox();
  if (!appearanceBox) throw new Error("appearance control has no layout box");
  expect(appearanceBox.x).toBeGreaterThanOrEqual(0);
  expect(appearanceBox.x + appearanceBox.width).toBeLessThanOrEqual(390);
  await appearance.click();
  const autoForMessage = page.getByRole("menuitemradio", { name: "Auto for this message" });
  const menuBox = await autoForMessage.boundingBox();
  if (!menuBox) throw new Error("appearance menu item has no layout box");
  expect(menuBox.x).toBeGreaterThanOrEqual(0);
  expect(menuBox.x + menuBox.width).toBeLessThanOrEqual(390);
  await page.keyboard.press("Escape");

  // The frame sizes to its content, not to a fixed default.
  await expect
    .poll(() =>
      page.getByTestId("thread-scroll").evaluate((scrollNode) => {
        const frameNode = scrollNode.querySelector("iframe");
        const doc = frameNode?.contentDocument?.documentElement;
        if (!frameNode || !doc) throw new Error("message iframe not found");
        return Math.abs(frameNode.getBoundingClientRect().height - doc.scrollHeight);
      }),
    )
    .toBeLessThan(2);

  await expect(lightReader.getByRole("button", { name: /^to Planetary Escape/ })).toBeVisible();
  // The toolbar keeps the frequent verbs; the rest are in More.
  for (const name of ["Archive (e)", "Snooze (Z)", "More actions"]) {
    await expect(lightReader.getByRole("button", { name, exact: true })).toBeVisible();
  }
  await expect(lightReader.getByRole("button", { name: "Reply…" })).toBeVisible();
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
  await page.getByRole("option", { name: /attachment smoke/ }).click();

  await page.getByRole("button", { name: "Open report.pdf" }).click();
  await expect
    .poll(() => openBody)
    .toEqual({ message_id: "msg-attachment", attachment_id: "att-report" });

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

  // A one-line body sizes to its content, not to a fixed tall frame.
  expect(layout.frameHeight).toBeGreaterThan(40);
  // Attachments (with their count heading) sit under the body, not below a
  // fixed-height frame.
  expect(layout.gapBeforeAttachments).toBeLessThan(64);
});
