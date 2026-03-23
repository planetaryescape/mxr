import type { AccountOperationResponse, AccountsResponse } from "../../shared/types";
import { cn } from "../lib/cn";
import { formatJson } from "./formatters";
import { HeaderActionButton } from "./shared";

export function AccountsWorkspace(props: {
  accounts: AccountsResponse["accounts"];
  selectedAccountId: string | null;
  status: string | null;
  result: AccountOperationResponse["result"] | null;
  onSelect: (accountId: string) => void;
  onNew: () => void;
  onTest: () => void;
  onSetDefault: () => void;
}) {
  const selectedAccount =
    props.accounts.find((account) => account.account_id === props.selectedAccountId) ?? null;

  return (
    <div className="grid h-full min-h-0 grid-cols-1 xl:grid-cols-[22rem_minmax(0,1fr)]">
      <section className="subtle-scrollbar min-h-0 overflow-y-auto border-r border-outline bg-panel px-4 py-4">
        <div className="flex items-center justify-between gap-3 border-b border-outline pb-3">
          <div>
            <p className="mono-meta">Accounts</p>
            <h1 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">Accounts</h1>
          </div>
          <HeaderActionButton label="New" onClick={props.onNew} />
        </div>
        {props.status ? (
          <div className="mt-3 border border-outline bg-panel-elevated px-3 py-2 text-sm text-foreground-muted">
            {props.status}
          </div>
        ) : null}
        <div className="mt-3 space-y-px">
          {props.accounts.length === 0 ? (
            <div className="border border-outline bg-panel-elevated px-3 py-3 text-sm text-foreground-muted">
              No accounts configured.
            </div>
          ) : (
            props.accounts.map((account) => (
              <button
                key={account.account_id}
                type="button"
                className={cn(
                  "w-full border px-3 py-2.5 text-left",
                  props.selectedAccountId === account.account_id
                    ? "border-outline-strong bg-panel-elevated"
                    : "border-transparent bg-transparent hover:bg-panel-elevated/55",
                )}
                onClick={() => props.onSelect(account.account_id)}
              >
                <div className="flex items-center justify-between gap-3">
                  <h2 className="text-sm font-medium text-foreground">{account.name}</h2>
                  <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-foreground-subtle">
                    {account.provider_kind}
                  </span>
                </div>
                <p className="mt-1 text-[12px] text-foreground-muted">{account.email}</p>
              </button>
            ))
          )}
        </div>
      </section>
      <section className="subtle-scrollbar min-h-0 overflow-y-auto bg-panel-muted px-4 py-4">
        <div className="flex flex-wrap items-start justify-between gap-3 border-b border-outline pb-3">
          <div>
            <p className="mono-meta">Account details</p>
            <h2 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">
              {selectedAccount?.name ?? "Select an account"}
            </h2>
          </div>
          <div className="flex flex-wrap gap-2">
            <HeaderActionButton label="Test" disabled={!selectedAccount} onClick={props.onTest} />
            <HeaderActionButton
              label="Set default"
              disabled={!selectedAccount || !selectedAccount.key}
              onClick={props.onSetDefault}
            />
          </div>
        </div>
        <div className="mt-4 grid gap-4 xl:grid-cols-[minmax(0,1fr)_20rem]">
          <div className="border border-outline bg-panel px-4 py-4">
            <pre className="whitespace-pre-wrap text-sm leading-6 text-foreground-muted">
              {formatJson(selectedAccount)}
            </pre>
          </div>
          <div className="border border-outline bg-panel px-4 py-4">
            <p className="mono-meta">Last operation</p>
            <pre className="mt-3 whitespace-pre-wrap text-sm leading-6 text-foreground-muted">
              {formatJson(props.result)}
            </pre>
          </div>
        </div>
      </section>
    </div>
  );
}
