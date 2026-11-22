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
  byId: ReadonlyMap<string, AccountIdentity>;
  accountsById: ReadonlyMap<string, RuntimeAccount>;
  several: boolean;
  marksShown: boolean;
}

const NONE: AccountIdentities = {
  byId: new Map(),
  accountsById: new Map(),
  several: false,
  marksShown: false,
};

const AccountIdentitiesContext = createContext<AccountIdentities>(NONE);

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

export function useRowAccountMark(
  accountId: string | null | undefined,
): AccountIdentity | undefined {
  const { byId, marksShown } = useAccountIdentities();
  return marksShown && accountId ? byId.get(accountId) : undefined;
}

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
      slot: isAccountSlot(identity.color) ? identity.color : "slate",
    });
  }
  const several = list.filter((account) => account.enabled).length > 1;
  return { byId, accountsById, several, marksShown: byId.size > 0 && several && !scoped };
}

export { AccountIdentitiesContext };
