import { apiFetch } from "@/api/client";

export interface RuleForm {
  id?: string | null;
  name: string;
  condition: string;
  action: string;
  priority: number;
  enabled: boolean;
}

export interface RuleListItem extends Partial<RuleForm> {
  rule?: string;
  last_fired_at?: string;
  fire_count?: number;
  [key: string]: unknown;
}

export function fetchRules() {
  return apiFetch<{ rules: RuleListItem[] }>("/api/v1/platform/rules");
}

export function fetchRuleForm(rule: string) {
  return apiFetch<{ form: RuleForm }>(
    `/api/v1/platform/rules/form?rule=${encodeURIComponent(rule)}`,
  );
}

export function fetchRuleHistory(rule: string) {
  return apiFetch<{ entries: unknown[] }>(
    `/api/v1/platform/rules/history?rule=${encodeURIComponent(rule)}`,
  );
}

export function dryRunRule(rule: string) {
  return apiFetch<{ results: unknown[] }>(
    `/api/v1/platform/rules/dry-run?rule=${encodeURIComponent(rule)}`,
  );
}

export function upsertRuleForm(form: RuleForm, existingRule?: string | null) {
  return apiFetch<{ rule: unknown }>("/api/v1/platform/rules/upsert-form", {
    method: "POST",
    body: {
      existing_rule: existingRule,
      name: form.name,
      condition: form.condition,
      action: form.action,
      priority: form.priority,
      enabled: form.enabled,
    },
  });
}

export function deleteRule(rule: string) {
  return apiFetch<{ ok: boolean }>("/api/v1/platform/rules/delete", {
    method: "POST",
    body: { rule },
  });
}
