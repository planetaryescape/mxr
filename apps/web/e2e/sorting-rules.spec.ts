import { expect, test } from "@playwright/test";
import { bridge, openApp } from "./helpers/state";
import type { RuleForm, TreatmentPreview } from "../src/features/rules/api";

test("sorting draft preview, apply and a preserved personal correction share daemon decisions", async ({
  page,
}) => {
  const accounts = await bridge<{ accounts: { account_id: string }[] }>(
    page,
    "/api/v1/platform/accounts",
  );
  const account = accounts.accounts[0].account_id;
  await openApp(page, "/rules/new");
  await page.getByLabel("Account", { exact: true }).selectOption(account);
  await page.getByLabel("Name", { exact: true }).fill("Sorting acceptance");
  await page.getByLabel("When a message matches").fill("from:@");
  await page.getByRole("button", { name: "Sort into Reading", exact: true }).click();
  await expect(page.getByText(/historical placements remain/)).toBeVisible();
  const form: RuleForm = {
    account_id: account,
    name: "Sorting acceptance",
    condition: "from:@",
    action: "treatment:reading",
    priority: 100,
    enabled: true,
  };
  const preview = await bridge<TreatmentPreview>(page, "/api/v1/platform/rules/treatment", {
    form,
  });
  expect(preview.result.matches.length).toBeGreaterThan(0);
  const applyButton = page.getByRole("button", { name: /^Apply to \d+ messages?$/ });
  await expect(applyButton).toBeEnabled();
  await applyButton.click();
  await page.getByRole("button", { name: "Apply now", exact: true }).click();
  await expect(page).not.toHaveURL(/\/rules\/new/);
  const message = preview.result.matches[0].message_id;
  await bridge(page, `/api/v1/mail/messages/${message}/move`, { mode: "messages", sender: false });
  const fresh = await bridge<TreatmentPreview>(page, "/api/v1/platform/rules/treatment", { form });
  const blocked = fresh.result.matches.find((row) => row.message_id === message);
  expect(blocked?.after).toBe("messages");
  expect(blocked?.blocked).toBe(true);
});
