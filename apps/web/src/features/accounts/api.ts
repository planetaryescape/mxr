import { apiFetch } from "@/api/client";
import type { RuntimeAccount } from "@/features/compose/api";

export interface AccountConfig {
  key: string;
  name: string;
  email: string;
  enabled: boolean;
  is_default: boolean;
  sync?: unknown;
  send?: unknown;
}

export interface AccountAddress {
  email: string;
  primary?: boolean;
  created_at?: string;
}

export interface AuthSession {
  session_id: string;
  state: "starting" | "waiting_for_user" | "authorized" | "failed" | "cancelled" | string;
  auth_url?: string;
  user_code?: string;
  verification_uri?: string;
  poll_interval_secs?: number;
  message?: string;
  error?: string;
}

export function fetchAccounts() {
  return apiFetch<{ accounts: RuntimeAccount[] }>("/api/v1/platform/accounts");
}

export function fetchAccountConfigs() {
  return apiFetch<{ accounts: AccountConfig[] }>("/api/v1/platform/accounts/config");
}

export function testAccount(account: AccountConfig) {
  return apiFetch<{ result: { ok: boolean; summary: string; [key: string]: unknown } }>(
    "/api/v1/platform/accounts/test",
    {
      method: "POST",
      body: account,
    },
  );
}

export function upsertAccount(account: AccountConfig) {
  return apiFetch<{ result: { ok: boolean; summary: string; [key: string]: unknown } }>(
    "/api/v1/platform/accounts/upsert",
    {
      method: "POST",
      body: account,
    },
  );
}

export function setDefaultAccount(key: string) {
  return apiFetch<unknown>("/api/v1/platform/accounts/default", { method: "POST", body: { key } });
}

export function repairAccount(account: AccountConfig): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>("/api/v1/platform/accounts/repair", {
    method: "POST",
    body: account,
  });
}

export function disableAccount(key: string) {
  return apiFetch<unknown>(`/api/v1/platform/accounts/${encodeURIComponent(key)}/disable`, {
    method: "POST",
  });
}

export function removeAccount(key: string, purgeLocalData = false) {
  return apiFetch<unknown>(
    `/api/v1/platform/accounts/${encodeURIComponent(key)}?purge_local_data=${purgeLocalData}`,
    { method: "DELETE" },
  );
}

/**
 * Pick the OAuth flow for an account's provider.
 *
 * Gmail uses a Desktop-app OAuth client, which Google rejects at the
 * device-code endpoint ("invalid_client: Invalid client type"); it must
 * use the loopback (Installed) flow, which the daemon resolves from
 * "auto". Outlook's adapter implements only the device-code flow, so it
 * stays on "device". (IMAP/SMTP never starts an OAuth session.)
 */
function authFlowForAccount(account: AccountConfig): "auto" | "device" {
  const syncType = (account.sync as { type?: string } | undefined)?.type;
  return syncType === "outlook_personal" ? "device" : "auto";
}

export function startAuthSession(account: AccountConfig, reauthorize = false) {
  return apiFetch<{ session: AuthSession }>("/api/v1/platform/auth/sessions/start", {
    method: "POST",
    body: { account, reauthorize, flow: authFlowForAccount(account) },
  });
}

export function fetchAuthSession(sessionId: string) {
  return apiFetch<{ session: AuthSession }>(
    `/api/v1/platform/auth/sessions/${encodeURIComponent(sessionId)}`,
  );
}

export function completeAuthSession(sessionId: string) {
  return apiFetch<{ session: AuthSession }>(
    `/api/v1/platform/auth/sessions/${encodeURIComponent(sessionId)}/complete`,
    {
      method: "POST",
      body: { save_account: true },
    },
  );
}

export function cancelAuthSession(sessionId: string) {
  return apiFetch<unknown>(
    `/api/v1/platform/auth/sessions/${encodeURIComponent(sessionId)}/cancel`,
    { method: "POST" },
  );
}

export function fetchAccountAddresses(accountId: string) {
  return apiFetch<{ addresses: AccountAddress[] }>(
    `/api/v1/platform/accounts/${accountId}/addresses`,
  );
}

export function addAccountAddress(accountId: string, email: string, primary = false) {
  return apiFetch<unknown>(`/api/v1/platform/accounts/${accountId}/addresses`, {
    method: "POST",
    body: { email, primary },
  });
}

export function removeAccountAddress(accountId: string, email: string) {
  return apiFetch<unknown>(`/api/v1/platform/accounts/${accountId}/addresses/remove`, {
    method: "POST",
    body: { email },
  });
}

export function setPrimaryAccountAddress(accountId: string, email: string) {
  return apiFetch<unknown>(`/api/v1/platform/accounts/${accountId}/addresses/primary`, {
    method: "POST",
    body: { email },
  });
}

export function gmailAccountConfig(email: string): AccountConfig {
  const safe = email || "gmail";
  return {
    key: `gmail-${safe}`,
    name: safe,
    email: safe,
    enabled: true,
    is_default: true,
    sync: {
      type: "gmail",
      credential_source: "bundled",
      client_id: "",
      client_secret: null,
      token_ref: `gmail:${safe}`,
    },
    send: { type: "gmail" },
  };
}

export function outlookAccountConfig(email: string): AccountConfig {
  const safe = email || "outlook";
  return {
    key: `outlook-${safe}`,
    name: safe,
    email: safe,
    enabled: true,
    is_default: true,
    sync: { type: "outlook_personal", client_id: null, token_ref: `outlook:${safe}` },
    send: { type: "outlook_personal", client_id: null, token_ref: `outlook:${safe}` },
  };
}

export function imapAccountConfig(input: {
  name: string;
  email: string;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  username: string;
  password: string;
}): AccountConfig {
  const passwordRef = `imap:${input.email}`;
  return {
    key: `imap-${input.email}`,
    name: input.name || input.email,
    email: input.email,
    enabled: true,
    is_default: true,
    sync: {
      type: "imap",
      host: input.imapHost,
      port: input.imapPort,
      username: input.username,
      password_ref: passwordRef,
      password: input.password,
      auth_required: true,
      use_tls: true,
    },
    send: {
      type: "smtp",
      host: input.smtpHost,
      port: input.smtpPort,
      username: input.username,
      password_ref: passwordRef,
      password: input.password,
      auth_required: true,
      use_tls: true,
    },
  };
}
