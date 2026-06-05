import { Cloud, MoreHorizontal, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { RuntimeAccount } from "./api";

interface ComposeTopBarProps {
  title: string;
  busy: boolean;
  canServerSave: boolean;
  onRefresh: () => void;
  onServerSave: () => void;
  onDiscard: () => void;
  accounts: RuntimeAccount[];
  accountId: string;
  onAccountChange: (id: string) => void;
}

export function ComposeTopBar({
  title,
  busy,
  canServerSave,
  onRefresh,
  onServerSave,
  onDiscard,
  accounts,
  accountId,
  onAccountChange,
}: ComposeTopBarProps) {
  return (
    <header className="shrink-0 border-b border-border">
      <div className="mx-auto flex h-14 w-full max-w-[860px] items-center justify-between gap-3 px-5">
        <div className="min-w-0">
          <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
            Compose
          </div>
          <h1 className="truncate text-sm font-semibold tracking-tight">{title}</h1>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <FromControl accounts={accounts} value={accountId} onChange={onAccountChange} />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label="More compose actions">
                <MoreHorizontal className="size-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-52">
              <DropdownMenuItem disabled={busy} onSelect={() => onRefresh()}>
                <RefreshCw className="size-3.5" />
                Refresh from daemon
                <DropdownMenuShortcut>⇧⌘R</DropdownMenuShortcut>
              </DropdownMenuItem>
              {canServerSave ? (
                <DropdownMenuItem disabled={busy} onSelect={() => onServerSave()}>
                  <Cloud className="size-3.5" />
                  Save to server draft
                  <DropdownMenuShortcut>⇧⌘S</DropdownMenuShortcut>
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuSeparator />
              <DropdownMenuItem
                disabled={busy}
                onSelect={() => onDiscard()}
                className="text-destructive focus:text-destructive"
              >
                <Trash2 className="size-3.5" />
                Discard draft
                <DropdownMenuShortcut>⌘⌫</DropdownMenuShortcut>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </header>
  );
}

function FromControl({
  accounts,
  value,
  onChange,
}: {
  accounts: RuntimeAccount[];
  value: string;
  onChange: (id: string) => void;
}) {
  if (accounts.length === 0) return null;
  const selected = accounts.find((account) => account.account_id === value) ?? accounts[0];
  if (accounts.length === 1) {
    return (
      <span
        className="hidden max-w-[220px] truncate font-mono text-2xs text-muted-foreground sm:inline"
        title={selected?.email}
      >
        {selected?.email}
      </span>
    );
  }
  return (
    <Select value={value || accounts[0]?.account_id} onValueChange={onChange}>
      <SelectTrigger className="h-8 w-[200px] bg-card text-xs" aria-label="Send from account">
        <SelectValue placeholder="From" />
      </SelectTrigger>
      <SelectContent>
        {accounts.map((account) => (
          <SelectItem key={account.account_id} value={account.account_id}>
            {account.name || account.email} · {account.email}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
