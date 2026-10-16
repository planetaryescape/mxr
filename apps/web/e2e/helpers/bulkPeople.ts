import type { Page } from "@playwright/test";

export const BULK_PEOPLE = 5_000;

/**
 * Messages with 5,000 people in Recent, grown from a real row so every field
 * the row renders has the daemon's shape (spike S4, docs/blueprint/31). A
 * synthetic person's page opens the real one it was grown from. The daemon
 * caps Recent in production; this stands in for the long lists W3 puts on
 * `Row`, to measure what moving the lamp costs when the list is long.
 */
export async function stubBulkPeople(page: Page): Promise<void> {
  let templateId = "";
  await page.route("**/api/v1/mail/people/page?*", async (route) => {
    const url = route
      .request()
      .url()
      .replace(/bulk-person-\d+/g, encodeURIComponent(templateId));
    await route.fulfill({ response: await route.fetch({ url }) });
  });
  // With no account or turn the daemon is asked without a query string.
  await page.route(/\/api\/v1\/mail\/people(\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    const template = json.messages.recent[0] ?? json.messages.your_turn[0];
    templateId = encodeURIComponent(template.id);
    json.messages.recent = Array.from({ length: BULK_PEOPLE }, (_, index) => ({
      ...template,
      id: `bulk-person-${index}`,
      title: `Bulk person ${index + 1}`,
      band: "recent",
      your_turn: false,
    }));
    json.messages.recent_total = BULK_PEOPLE;
    // Recent leads, so the first press lights bulk-person-0.
    json.messages.your_turn = [];
    json.messages.pinned = [];
    await route.fulfill({ response, json });
  });
}
