import { useEffectEvent } from "react";
import type { SetStateAction } from "react";
import type {
  AccountConfig,
  AccountOperationResponse,
  AccountsResponse,
  ActionAckResponse,
  BridgeState,
  FocusContext,
  RuleDetailResponse,
  RuleDryRunResponse,
  RuleFormPayload,
  RuleFormResponse,
  RuleHistoryResponse,
  RulesResponse,
} from "../../shared/types";
import { fetchJson } from "./bridgeHttp";
import type { DesktopRequestCoordinator } from "./requestCoordinator";

type StateSetter<T> = (updater: SetStateAction<T>) => void;

export function useRulesAccountsActions(props: {
  requestCoordinator: DesktopRequestCoordinator;
  bridge: BridgeState;
  selectedRuleId: string | null;
  selectedRule: RulesResponse["rules"][number] | null;
  selectedAccount: AccountsResponse["accounts"][number] | null;
  ruleFormState: RuleFormPayload;
  accountDraftJson: string;
  accountFormOpen: boolean;
  setFocusContext: StateSetter<FocusContext>;
  setRuleDetail: StateSetter<RuleDetailResponse["rule"] | null>;
  setRulePanelMode: StateSetter<"details" | "history" | "dryRun">;
  setRuleHistoryState: StateSetter<Array<Record<string, unknown>>>;
  setRuleDryRunState: StateSetter<Array<Record<string, unknown>>>;
  setRuleStatus: StateSetter<string | null>;
  setRuleFormOpen: StateSetter<boolean>;
  setRuleFormBusy: StateSetter<string | null>;
  setRuleFormState: StateSetter<RuleFormPayload>;
  setSelectedRuleId: StateSetter<string | null>;
  setAccountStatus: StateSetter<string | null>;
  setAccountResult: StateSetter<AccountOperationResponse["result"] | null>;
  setAccountFormOpen: StateSetter<boolean>;
  setAccountFormBusy: StateSetter<string | null>;
  setAccountDraftJson: StateSetter<string>;
  loadRules: () => Promise<void>;
  loadAccounts: () => Promise<void>;
  showNotice: (message: string) => void;
}) {
  const loadSelectedRuleDetail = useEffectEvent(async (ruleId?: string | null) => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready") {
      return;
    }
    const target = ruleId ?? props.selectedRuleId;
    if (!target) {
      props.setRuleDetail(null);
      return;
    }
    const params = new URLSearchParams({ rule: target });
    const path = `/rules/detail?${params.toString()}`;
    const result = await props.requestCoordinator.runReplaceable(
      `rules:detail:${target}`,
      ({ signal }) =>
        fetchJson<RuleDetailResponse>(bridge.baseUrl, bridge.authToken, path, {
          signal,
          requestLabel: "rules:detail",
        }),
    );
    if (result.status !== "committed") {
      return;
    }
    const payload = result.value;
    props.setRuleDetail(payload.rule);
    props.setRulePanelMode("details");
  });

  const openRuleHistory = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.selectedRuleId) {
      return;
    }
    const params = new URLSearchParams({ rule: props.selectedRuleId });
    const path = `/rules/history?${params.toString()}`;
    const result = await props.requestCoordinator.runReplaceable(
      `rules:history:${props.selectedRuleId}`,
      ({ signal }) =>
        fetchJson<RuleHistoryResponse>(bridge.baseUrl, bridge.authToken, path, {
          signal,
          requestLabel: "rules:history",
        }),
    );
    if (result.status !== "committed") {
      return;
    }
    const payload = result.value;
    props.setRuleHistoryState(payload.entries);
    props.setRulePanelMode("history");
  });

  const openRuleDryRun = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.selectedRuleId) {
      return;
    }
    const params = new URLSearchParams({ rule: props.selectedRuleId });
    const path = `/rules/dry-run?${params.toString()}`;
    const result = await props.requestCoordinator.runReplaceable(
      `rules:dry-run:${props.selectedRuleId}`,
      ({ signal }) =>
        fetchJson<RuleDryRunResponse>(bridge.baseUrl, bridge.authToken, path, {
          signal,
          requestLabel: "rules:dry-run",
        }),
    );
    if (result.status !== "committed") {
      return;
    }
    const payload = result.value;
    props.setRuleDryRunState(payload.results);
    props.setRulePanelMode("dryRun");
  });

  const openRuleForm = useEffectEvent(async (mode: "new" | "edit") => {
    if (mode === "new") {
      props.setRuleFormState({
        id: null,
        name: "",
        condition: "",
        action: "",
        priority: 100,
        enabled: true,
      });
      props.setRuleFormOpen(true);
      props.setFocusContext("dialog");
      return;
    }
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.selectedRuleId) {
      return;
    }
    const params = new URLSearchParams({ rule: props.selectedRuleId });
    const path = `/rules/form?${params.toString()}`;
    const result = await props.requestCoordinator.runReplaceable(
      `rules:form:${props.selectedRuleId}`,
      ({ signal }) =>
        fetchJson<RuleFormResponse>(bridge.baseUrl, bridge.authToken, path, {
          signal,
          requestLabel: "rules:form",
        }),
    );
    if (result.status !== "committed") {
      return;
    }
    const payload = result.value;
    props.setRuleFormState(payload.form);
    props.setRuleFormOpen(true);
    props.setFocusContext("dialog");
  });

  const saveRuleForm = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready") {
      return;
    }
    props.setRuleFormBusy("Saving");
    try {
      const payload = await props.requestCoordinator.enqueueMutation(() =>
        fetchJson<RuleDetailResponse>(bridge.baseUrl, bridge.authToken, "/rules/upsert-form", {
          method: "POST",
          body: JSON.stringify({
            existing_rule: props.ruleFormState.id,
            name: props.ruleFormState.name,
            condition: props.ruleFormState.condition,
            action: props.ruleFormState.action,
            priority: props.ruleFormState.priority,
            enabled: props.ruleFormState.enabled,
          }),
          requestLabel: "rules:upsert-form",
        }),
      );
      props.setRuleDetail(payload.rule);
      props.setSelectedRuleId(String(payload.rule.id ?? payload.rule.name ?? ""));
      props.setRulePanelMode("details");
      props.setRuleFormOpen(false);
      props.setRuleStatus("Rule saved");
      await props.loadRules();
    } finally {
      props.setRuleFormBusy(null);
    }
  });

  const toggleSelectedRuleEnabled = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.selectedRule) {
      return;
    }
    const enabled = Boolean(props.selectedRule.enabled);
    const payload = await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<RuleDetailResponse>(bridge.baseUrl, bridge.authToken, "/rules/upsert", {
        method: "POST",
        body: JSON.stringify({
          rule: {
            ...props.selectedRule,
            enabled: !enabled,
          },
        }),
        requestLabel: "rules:toggle",
      }),
    );
    props.setRuleDetail(payload.rule);
    props.setRuleStatus(enabled ? "Rule disabled" : "Rule enabled");
    await props.loadRules();
  });

  const deleteSelectedRule = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.selectedRuleId) {
      return;
    }
    await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<ActionAckResponse>(bridge.baseUrl, bridge.authToken, "/rules/delete", {
        method: "POST",
        body: JSON.stringify({ rule: props.selectedRuleId }),
        requestLabel: "rules:delete",
      }),
    );
    props.setRuleDetail(null);
    props.setRuleHistoryState([]);
    props.setRuleDryRunState([]);
    props.setRuleStatus("Rule deleted");
    await props.loadRules();
  });

  const openAccountForm = useEffectEvent(() => {
    props.setAccountDraftJson(JSON.stringify(defaultAccountTemplate(), null, 2));
    props.setAccountFormOpen(true);
    props.setFocusContext("dialog");
  });

  const testCurrentAccount = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready") {
      return;
    }
    let account: AccountConfig | null;
    try {
      account = props.accountFormOpen
        ? parseAccountConfigDraft(props.accountDraftJson)
        : accountSummaryToConfig(props.selectedAccount);
    } catch (error) {
      props.setAccountStatus(error instanceof Error ? error.message : "Invalid account JSON");
      return;
    }
    if (!account) {
      props.showNotice("No editable account selected");
      return;
    }
    props.setAccountFormBusy("Testing");
    try {
      const payload = await props.requestCoordinator.enqueueMutation(() =>
        fetchJson<AccountOperationResponse>(bridge.baseUrl, bridge.authToken, "/accounts/test", {
          method: "POST",
          body: JSON.stringify(account),
          requestLabel: "accounts:test",
        }),
      );
      props.setAccountResult(payload.result);
      props.setAccountStatus(payload.result.summary);
    } finally {
      props.setAccountFormBusy(null);
    }
  });

  const saveAccountDraft = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready") {
      return;
    }
    let account: AccountConfig;
    try {
      account = parseAccountConfigDraft(props.accountDraftJson);
    } catch (error) {
      props.setAccountStatus(error instanceof Error ? error.message : "Invalid account JSON");
      return;
    }
    props.setAccountFormBusy("Saving");
    try {
      const payload = await props.requestCoordinator.enqueueMutation(() =>
        fetchJson<AccountOperationResponse>(bridge.baseUrl, bridge.authToken, "/accounts/upsert", {
          method: "POST",
          body: JSON.stringify(account),
          requestLabel: "accounts:upsert",
        }),
      );
      props.setAccountResult(payload.result);
      props.setAccountStatus(payload.result.summary);
      props.setAccountFormOpen(false);
      await props.loadAccounts();
    } finally {
      props.setAccountFormBusy(null);
    }
  });

  const makeSelectedAccountDefault = useEffectEvent(async () => {
    const bridge = props.bridge;
    const selectedAccount = props.selectedAccount;
    if (bridge.kind !== "ready" || !selectedAccount?.key) {
      props.showNotice("Selected account cannot be set default");
      return;
    }
    const payload = await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<AccountOperationResponse>(bridge.baseUrl, bridge.authToken, "/accounts/default", {
        method: "POST",
        body: JSON.stringify({ key: selectedAccount.key }),
        requestLabel: "accounts:default",
      }),
    );
    props.setAccountResult(payload.result);
    props.setAccountStatus(payload.result.summary);
    await props.loadAccounts();
  });

  return {
    loadSelectedRuleDetail,
    openRuleHistory,
    openRuleDryRun,
    openRuleForm,
    saveRuleForm,
    toggleSelectedRuleEnabled,
    deleteSelectedRule,
    openAccountForm,
    testCurrentAccount,
    saveAccountDraft,
    makeSelectedAccountDefault,
  };
}

function defaultAccountTemplate(): AccountConfig {
  return {
    key: "personal",
    name: "Personal",
    email: "me@example.com",
    is_default: true,
    sync: {
      type: "gmail",
      credential_source: "bundled",
      client_id: "",
      client_secret: null,
      token_ref: "gmail:personal",
    },
    send: {
      type: "gmail",
    },
  };
}

function parseAccountConfigDraft(draftJson: string) {
  return JSON.parse(draftJson) as AccountConfig;
}

function accountSummaryToConfig(
  account: AccountsResponse["accounts"][number] | null,
): AccountConfig | null {
  if (!account?.key) {
    return null;
  }
  if (!account.sync && !account.send) {
    return null;
  }
  return {
    key: account.key,
    name: account.name,
    email: account.email,
    is_default: account.is_default,
    sync: account.sync ?? null,
    send: account.send ?? null,
  };
}
