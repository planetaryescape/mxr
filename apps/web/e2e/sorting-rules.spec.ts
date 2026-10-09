import { expect, test } from "@playwright/test";
import { bridge, openApp } from "./helpers/state";
import { deliver } from "./helpers/spool";
import type { RuleForm, TreatmentPreview } from "../src/features/rules/api";

test("sorting draft preview, apply and a preserved personal correction share daemon decisions", async ({
  page,
}) => {
  const accounts = await bridge<{ accounts: { account_id: string }[] }>(
    page,
    "/api/v1/platform/accounts",
  );
  const account = accounts.accounts[0]?.account_id;
  if (!account) throw new Error("Synthetic sorting account missing");
  const unique = `sorting-${Date.now()}`;
  const sender = `${unique}@sorting.example.com`;
  const name = `Sorting acceptance ${unique}`;
  deliver({
    from: sender,
    to: "alex@demo.mxr.local",
    subject: unique,
    body: "Synthetic sorting acceptance mail",
  });
  await bridge(page, "/api/v1/mail/sync", {});
  const form: RuleForm = {
    account_id: account,
    name,
    condition: `from:${sender}`,
    action: "treatment:reading",
    priority: 100,
    enabled: true,
  };
  let preview: TreatmentPreview | undefined;
  await expect
    .poll(async () => {
      preview = await bridge<TreatmentPreview>(page, "/api/v1/platform/rules/treatment", { form });
      return preview.result.matches.length;
    })
    .toBe(1);
  const message = preview?.result.matches[0]?.message_id;
  if (!message) throw new Error("Injected sorting message did not arrive");
  let ruleId: string | undefined;
  let correctionId: number | undefined;
  try {
    await openApp(page, "/rules/new");
    await page.getByLabel("Account", { exact: true }).selectOption(account);
    await page.getByLabel("Name", { exact: true }).fill(name);
    await page.getByLabel("When a message matches").fill(form.condition);
    await page.getByRole("button", { name: "Sort into Reading", exact: true }).click();
    await expect(page.getByText(/historical placements remain/)).toBeVisible();
    const applyButton = page.getByRole("button", { name: /^Apply to 1 message$/ });
    await expect(applyButton).toBeEnabled();
    await applyButton.click();
    const response = page.waitForResponse(
      (r) =>
        r.url().endsWith("/rules/treatment") && Boolean(r.request().postDataJSON()?.preview_token),
    );
    await page.getByRole("button", { name: "Apply now", exact: true }).click();
    const applied: TreatmentPreview = await (await response).json();
    if (!applied.rule_id) throw new Error("Sorting apply did not return a saved rule");
    ruleId = applied.rule_id;
    expect(applied.result.matches.map((row) => row.message_id)).toEqual([message]);
    await expect(page).not.toHaveURL(/\/rules\/new/);
    const moved = await bridge<{ outcome: { correction_id: number } }>(
      page,
      `/api/v1/mail/messages/${message}/move`,
      { mode: "messages", sender: false },
    );
    correctionId = moved.outcome.correction_id;
    const fresh = await bridge<TreatmentPreview>(page, "/api/v1/platform/rules/treatment", {
      form: { ...form, id: ruleId },
    });
    expect(fresh.result.matches[0]?.after).toBe("messages");
    expect(fresh.result.matches[0]?.blocked).toBe(true);
  } finally {
    if (correctionId) await bridge(page, `/api/v1/mail/moves/${correctionId}/undo`, {});
    if (ruleId) await bridge(page, "/api/v1/platform/rules/delete", { rule: ruleId });
    // Historical sorting remains by design; archive only this test's mail.
    await bridge(page, "/api/v1/mail/mutations/archive", { message_ids: [message] });
  }
});
