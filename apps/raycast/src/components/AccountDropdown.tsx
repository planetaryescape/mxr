import { Icon, List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { getAccounts } from "../bridge/mail";

/** The dropdown's value for every account; the bridge reads no `account` as all. */
const ALL = "all";

/**
 * The account filter in the search bar; Raycast opens it with ⌘P. Each
 * command remembers its own choice.
 */
export function AccountDropdown({ onChange }: { onChange: (account?: string) => void }) {
  const { data: accounts } = useCachedPromise(getAccounts, [], { keepPreviousData: true });
  return (
    <List.Dropdown
      tooltip="Account"
      storeValue
      onChange={(value) => onChange(value === ALL ? undefined : value)}
    >
      <List.Dropdown.Item title="All Accounts" value={ALL} icon={Icon.Tray} />
      {accounts && accounts.length > 1 && (
        <List.Dropdown.Section>
          {accounts.map((account) => (
            <List.Dropdown.Item
              key={account.account_id}
              title={
                account.name === account.email
                  ? account.email
                  : `${account.name} · ${account.email}`
              }
              keywords={[account.email, account.key ?? ""]}
              value={account.account_id}
              icon={Icon.Person}
            />
          ))}
        </List.Dropdown.Section>
      )}
    </List.Dropdown>
  );
}
