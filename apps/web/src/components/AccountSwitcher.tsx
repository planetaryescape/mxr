import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Check, ChevronsUpDown, Layers, Settings2, UserPlus } from "lucide-react";
import { useEffect } from "react";

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
import { initials } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * Scopes every mail view to one account, or all of them. Unlike the TUI,
 * which switches by rewriting the default account, this changes nothing
 * on disk: it only filters what the web shows.
 */
export function AccountSwitcher({ collapsed = false }: { collapsed?: boolean }) {
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts, staleTime: 60_000 });
  const scope = useUiPrefs((s) => s.accountScope);
  const setScope = useUiPrefs((s) => s.setAccountScope);
  const rows = (accounts.data?.accounts ?? []).filter((row) => row.enabled !== false);
  const scoped = rows.find((row) => row.account_id === scope);
  const title = scoped
    ? scoped.name || scoped.email
    : rows.length > 1
      ? "All accounts"
      : (rows[0]?.name ?? "mxr");
  // A scope pointing at a removed or disabled account would hide all mail.
  useEffect(() => {
    if (scope && accounts.isSuccess && !scoped) setScope(null);
  }, [accounts.isSuccess, scope, scoped, setScope]);
  const subtitle = scoped
    ? scoped.email
    : rows.length > 1
      ? `${rows.length} accounts`
      : (rows[0]?.email ?? "");

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          className={cn(
            "h-10 w-full justify-start gap-2 px-2 text-left",
            collapsed && "justify-center px-0",
          )}
          aria-label={`Account: ${title}. Switch account`}
        >
          <span className="grid size-7 shrink-0 place-items-center rounded-md bg-primary-muted font-mono text-[11px] font-semibold text-primary">
            {scoped || rows.length === 1 ? initials(title) : <Layers className="size-3.5" />}
          </span>
          {!collapsed && (
            <>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] font-semibold leading-tight">
                  {title}
                </span>
                {subtitle ? (
                  <span className="block truncate font-mono text-2xs text-muted-foreground">
                    {subtitle}
                  </span>
                ) : null}
              </span>
              <ChevronsUpDown className="size-3.5 shrink-0 text-muted-foreground" />
            </>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        <DropdownMenuLabel className="font-mono text-2xs uppercase tracking-wider text-muted-foreground">
          Show mail from
        </DropdownMenuLabel>
        {accounts.isLoading ? (
          <DropdownMenuItem disabled>Loading accounts…</DropdownMenuItem>
        ) : null}
        {rows.length > 1 ? (
          <DropdownMenuItem onSelect={() => setScope(null)}>
            <Layers className="size-3.5" />
            <span className="flex-1">All accounts</span>
            {scope === null ? <Check className="size-3.5 text-primary" /> : null}
          </DropdownMenuItem>
        ) : null}
        {rows.map((row) => (
          <DropdownMenuItem key={row.account_id} onSelect={() => setScope(row.account_id)}>
            <span className="grid size-5 place-items-center rounded bg-muted font-mono text-[10px]">
              {initials(row.name || row.email)}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate">{row.name || row.email}</span>
              <span className="block truncate font-mono text-2xs text-muted-foreground">
                {row.email}
              </span>
            </span>
            {scope === row.account_id || (scope === null && rows.length === 1) ? (
              <Check className="size-3.5 text-primary" />
            ) : null}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem asChild>
          <Link to="/accounts">
            <Settings2 className="size-3.5" /> Manage accounts
          </Link>
        </DropdownMenuItem>
        <DropdownMenuItem asChild>
          <Link to="/accounts/$key" params={{ key: "new" }}>
            <UserPlus className="size-3.5" /> Add account
          </Link>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
