import { ChevronDown, Mail, UserPlus } from "lucide-react";
import { useQuery } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { fetchAccounts } from "@/features/accounts/api";

export function AccountSwitcher({ collapsed = false }: { collapsed?: boolean }) {
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    staleTime: 60_000,
  });
  const rows = accounts.data?.accounts ?? [];
  const account =
    rows.find((row) => row.enabled && row.is_default) ??
    rows.find((row) => row.enabled) ??
    rows[0] ?? { name: "All accounts", email: "" };
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          className="h-9 w-full justify-start gap-2 px-2 text-left"
          aria-label="Account switcher"
        >
          <div className="flex size-6 shrink-0 items-center justify-center rounded-md bg-primary-muted text-primary">
            <Mail className="size-3" />
          </div>
          {!collapsed && (
            <>
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs font-medium leading-tight">{account.name}</div>
                {account.email ? (
                  <div className="truncate font-mono text-2xs text-muted-foreground">
                    {account.email}
                  </div>
                ) : null}
              </div>
              <ChevronDown className="size-3 shrink-0 opacity-60" />
            </>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60">
        <DropdownMenuLabel>Accounts</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {accounts.isLoading ? (
          <DropdownMenuItem disabled className="text-2xs text-muted-foreground">
            Loading accounts...
          </DropdownMenuItem>
        ) : rows.length === 0 ? (
          <DropdownMenuItem disabled className="text-2xs text-muted-foreground">
            No accounts loaded yet
          </DropdownMenuItem>
        ) : (
          rows.map((row) => (
            <DropdownMenuItem key={row.account_id} asChild>
              <a href={`/accounts/${encodeURIComponent(row.key ?? row.account_id)}`}>
                <Mail className="size-3" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{row.name || row.email}</span>
                  <span className="block truncate font-mono text-2xs text-muted-foreground">
                    {row.email}
                  </span>
                </span>
                {row.is_default ? (
                  <span className="rounded bg-primary-muted px-1 text-2xs text-primary">
                    default
                  </span>
                ) : null}
              </a>
            </DropdownMenuItem>
          ))
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem asChild>
          <a href="/accounts/new">
            <UserPlus className="size-3" /> Add account
          </a>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
