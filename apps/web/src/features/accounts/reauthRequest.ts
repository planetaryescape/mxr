import type { RuntimeAccount } from "@/features/compose/api";

const ACCOUNT_REAUTH_REQUEST_KEY = "mxr.account.reauth.request.v1";

export function requestAccountReauth(accountId: string) {
  if (typeof window === "undefined") return;
  window.sessionStorage.setItem(ACCOUNT_REAUTH_REQUEST_KEY, accountId);
}

export function claimAccountReauthRequest(account: Pick<RuntimeAccount, "account_id" | "key">) {
  if (typeof window === "undefined") return false;
  const requested = window.sessionStorage.getItem(ACCOUNT_REAUTH_REQUEST_KEY);
  if (!requested) return false;
  const matches = requested === account.account_id || requested === account.key;
  if (matches) {
    window.sessionStorage.removeItem(ACCOUNT_REAUTH_REQUEST_KEY);
  }
  return matches;
}
