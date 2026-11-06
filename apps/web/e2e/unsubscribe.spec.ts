import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Edition {
  bands: {
    band: string;
    items: {
      item_key: string;
      thread_id: string;
      title: string;
      links?: { item_key: string; title: string; domain: string }[];
    }[];
  }[];
  later_count: number;
}

async function edition(page: Page): Promise<Edition> {
  return (await bridge<{ edition: Edition }>(page, "/api/v1/mail/reading")).edition;
}

/** The demo's Reading mail is extracted on the first edition; wait for it. */
async function waitForEdition(page: Page): Promise<Edition> {
  let answer: Edition | undefined;
  await expect
    .poll(
      async () => {
        answer = await edition(page);
        return answer.bands.flatMap((band) => band.items).length;
      },
      { timeout: 60_000 },
    )
    .toBeGreaterThan(0);
  return answer!;
}

function items(page: Page) {
  return page.getByTestId("reading-item");
}

async function openEdition(page: Page) {
  await waitForEdition(page);
  await openApp(page, "/reading");
  await expect(items(page).first()).toBeVisible();
}

test("Unsubscribe commits exactly the daemon's preview, for both choices", async ({ page }) => {
  await openEdition(page);
  await items(page)
    .filter({ hasText: "Growth Digest" })
    .first()
    .click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Enter");
  const control = page.getByTestId("reader-unsubscribe");
  await expect(control).toBeInViewport();
  const commits: { preview_token: string; archive: boolean }[] = [];
  let previewToken = "";
  await page.route(
    /\/api\/v1\/mail\/actions\/unsubscribe(?:-purge|-preview\/commit)$/,
    async (route) => {
      const request = route.request().postDataJSON();
      if (request.dry_run) {
        const response = await route.fetch();
        const preview = await response.json();
        previewToken = preview.result.preview_token;
        await route.fulfill({ response });
      } else {
        // The outbound unsubscribe is irreversible; daemon tests use FakeProvider.
        // Here we check each button's real request against the real daemon preview.
        commits.push({ preview_token: request.preview_token, archive: request.archive });
        await route.fulfill({
          json: {
            ok: true,
            result: {
              address: request.address,
              method: "None",
              status: "unsubscribed",
              message_count: 12,
              archived_count: request.archive ? 12 : 0,
            },
          },
        });
      }
    },
  );
  await control.click();
  const dialog = page.getByTestId("reading-unsubscribe");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId("unsubscribe-just")).toContainText(
    "Just unsubscribe — keep what you have",
  );
  await expect(dialog.getByTestId("unsubscribe-clear")).toContainText(
    /Unsubscribe and clear \d+ issues?/,
  );
  await expect(dialog.getByTestId("unsubscribe-just")).toBeEnabled();
  const keepToken = previewToken;
  await dialog.getByTestId("unsubscribe-just").click();
  await expect(dialog).toHaveCount(0);
  expect(commits).toEqual([{ preview_token: keepToken, archive: false }]);
  await control.click();
  await expect(dialog.getByTestId("unsubscribe-clear")).toBeEnabled();
  const clearToken = previewToken;
  await dialog.getByTestId("unsubscribe-clear").click();
  await expect(dialog).toHaveCount(0);
  expect(commits).toEqual([
    { preview_token: keepToken, archive: false },
    { preview_token: clearToken, archive: true },
  ]);
});
