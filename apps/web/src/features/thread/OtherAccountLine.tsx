import { AccountMark } from "@/components/AccountMark";
import { useAccountIdentities } from "@/features/accounts/useAccountIdentities";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * The reader's account line (docs/blueprint/33): the chip, then "In Work · address".
 * With several accounts it always shows, so the reader never leaves which account
 * a message came to to guesswork. When the app is scoped to another account, it
 * also says the conversation is not the one being viewed. One-account users see
 * the line only in that scoped case.
 */
export function OtherAccountLine({ accountId }: { accountId: string }) {
  const scope = useUiPrefs((s) => s.accountScope);
  const accounts = useAccountIdentities();
  const account = accounts.accountsById.get(accountId);
  const elsewhere = scope !== null && scope !== accountId;
  if (!account || !(accounts.several || elsewhere)) return null;
  const identity = accounts.byId.get(accountId);
  return (
    <p
      data-testid="other-account-line"
      className="reader-band flex items-center gap-2 border-b border-border py-1.5 text-2xs text-muted-foreground"
    >
      {identity ? <AccountMark identity={identity} size="header" /> : null}
      <span>
        In {account.name} · {account.email}
        {elsewhere ? " · not the account you're viewing" : null}
      </span>
    </p>
  );
}
