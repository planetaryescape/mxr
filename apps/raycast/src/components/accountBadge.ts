import { Color, type List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { type Account, getAccounts } from "../bridge/mail";

const PALETTE = [
  Color.Blue,
  Color.Green,
  Color.Orange,
  Color.Purple,
  Color.Magenta,
  Color.Yellow,
  Color.Red,
];

function label(account: Account): string {
  return account.key || account.email.split("@")[0] || account.name;
}

export interface AccountTag {
  label: string;
  color: Color;
  email: string;
}

export function useAccountBadge(selected: string | undefined) {
  const { data: accounts } = useCachedPromise(getAccounts, [], { keepPreviousData: true });
  const tagOf = (accountId: string | null | undefined): AccountTag | undefined => {
    if (!accounts || !accountId) return undefined;
    const index = accounts.findIndex((account) => account.account_id === accountId);
    const account = accounts[index];
    if (!account) return undefined;
    return {
      label: label(account),
      color: PALETTE[index % PALETTE.length] ?? Color.Blue,
      email: account.email,
    };
  };
  const badge = (accountId: string | null | undefined): List.Item.Accessory[] => {
    if (selected || !accounts || accounts.length < 2) return [];
    const tag = tagOf(accountId);
    return tag ? [{ tag: { value: tag.label, color: tag.color }, tooltip: tag.email }] : [];
  };
  return Object.assign(badge, { tagOf });
}
