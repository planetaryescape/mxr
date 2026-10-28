import { useQuery } from "@tanstack/react-query";
import { createContext, useContext, useMemo, type ReactNode } from "react";

import type { RuntimeAccount } from "@/features/compose/api";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { isAccountSlot, type AccountSlot } from "@/styles/accountSlots";

import { fetchAccounts } from "./api";

export interface AccountIdentity {
  accountId: string;
  name: string;
  email: string;
  tag: string;
  slot: AccountSlot;
}

export interface AccountIdentities {
  /** Keyed by account id, never by position: the daemon's list order is not stable. */
  byId: ReadonlyMap<string, AccountIdentity>;
  /** Every account, identity or not, for the names and addresses that always show. */
  accountsById: ReadonlyMap<string, RuntimeAccount>;
  /** More than one enabled account. Places that always show the account draw its chip only then. */
  several: boolean;
  /**
   * Rows wear their account's mark when the view mixes accounts: more than one
   * enabled account, no single account chosen (docs/blueprint/33). An older
   * daemon sends no identities, so nothing is marked.
   */
  marksShown: boolean;
}

const NONE: AccountIdentities = {
  byId: new Map(),
  accountsById: new Map(),
  several: false,
  marksShown: false,
};

const AccountIdentitiesContext = createContext<AccountIdentities>(NONE);

/**
 * One accounts query for the whole app, so each row reads a context value
 * instead of running its own query. Mounted once at the root; a save that
 * invalidates ["accounts"] refreshes every mark with it.
 */
export function AccountIdentitiesProvider({ children }: { children: ReactNode }) {
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const scoped = useUiPrefs((s) => s.accountScope !== null);
  const list = accounts.data?.accounts;
  const value = useMemo(() => (list ? identitiesFrom(list, scoped) : NONE), [list, scoped]);
  return (
    <AccountIdentitiesContext.Provider value={value}>{children}</AccountIdentitiesContext.Provider>
  );
}

export function useAccountIdentities(): AccountIdentities {
  return useContext(AccountIdentitiesContext);
}

/** The derived view of an accounts list. `scoped` is true when the app shows one account. */
export function identitiesFrom(
  list: readonly RuntimeAccount[],
  scoped: boolean,
): AccountIdentities {
  const byId = new Map<string, AccountIdentity>();
  const accountsById = new Map<string, RuntimeAccount>();
  for (const account of list) {
    accountsById.set(account.account_id, account);
    const identity = account.identity;
    if (!identity) continue;
    byId.set(account.account_id, {
      accountId: account.account_id,
      name: account.name,
      email: account.email,
      tag:
        typeof identity.tag === "string" && identity.tag ? identity.tag : account.name.slice(0, 1),
      // An unknown slot from a newer or buggy daemon falls back to slate, never a crash.
      slot: isAccountSlot(identity.color) ? identity.color : "slate",
    });
  }
  const several = list.filter((account) => account.enabled).length > 1;
  return { byId, accountsById, several, marksShown: byId.size > 0 && several && !scoped };
}

export { AccountIdentitiesContext };
