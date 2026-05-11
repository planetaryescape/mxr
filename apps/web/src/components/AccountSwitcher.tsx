import { ChevronDown, Mail, UserPlus } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function AccountSwitcher({ collapsed = false }: { collapsed?: boolean }) {
  // Phase 8 wires this to /api/v1/platform/accounts. For Phase 1 it's a stub.
  const account = { name: "All accounts", email: "" };
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
        <DropdownMenuItem disabled className="text-2xs text-muted-foreground">
          No accounts loaded yet
        </DropdownMenuItem>
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
