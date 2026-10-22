import { Color, type List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { type Account, getAccounts } from "../bridge/mail";

/** One colour per account, in the daemon's account order, so it stays put. */
const PALETTE = [
  Color.Blue,
  Color.Green,
  Color.Orange,
  Color.Purple,
  Color.Magenta,
  Color.Yellow,
  Color.Red,
];

/** The short name: the config key ("work"), else the address before the @. */
function label(account: Account): string {
  return account.key || account.email.split("@")[0] || account.name;
}

/**
 * A coloured tag naming the account a row came from. Shown only while the
 * list covers every account and there is more than one; a filtered list is
 * all one account already.
 */
export function useAccountBadge(selected: string | undefined) {
  const { data: accounts } = useCachedPromise(getAccounts, [], { keepPreviousData: true });
  return (accountId: string | null | undefined): List.Item.Accessory[] => {
    if (selected || !accounts || accounts.length < 2 || !accountId) return [];
    const index = accounts.findIndex((account) => account.account_id === accountId);
    const account = accounts[index];
    if (!account) return [];
    return [
      {
        tag: { value: label(account), color: PALETTE[index % PALETTE.length] },
        tooltip: account.email,
      },
    ];
  };
}
