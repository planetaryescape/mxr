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

/** One rule run on one message (`mxr_store::row_to_rule_log_json`). */
export interface RuleHistoryEntry {
  rule_id: string;
  rule_name: string;
  message_id: string;
  actions_applied: string[];
  timestamp: string;
  success: boolean;
  error?: string | null;
}

/** `mxr_rules::DryRunResult`: one entry per rule, with the messages it would touch. */
export interface RuleDryRunResult {
  rule_id: string;
  rule_name: string;
  matches: { message_id: string; from: string; subject: string; actions: unknown[] }[];
}

export function fetchRuleHistory(rule: string) {
  return apiFetch<{ entries: RuleHistoryEntry[] }>(
    `/api/v1/platform/rules/history?rule=${encodeURIComponent(rule)}`,
  );
}

export function dryRunRule(rule: string) {
  return apiFetch<{ results: RuleDryRunResult[] }>(
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
