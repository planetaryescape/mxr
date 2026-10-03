import { Link, useNavigate } from "@tanstack/react-router";
import { FolderArchive, Inbox, Search } from "lucide-react";
import { useId, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/**
 * Find, the phone's fifth tab: search all mail, Archive (whose answer box
 * is a search) and the Inbox, everything newest first.
 */
export function FindRoute() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const inputId = useId();
  return (
    <section aria-label="Find" className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
      <header className="shrink-0 border-b border-border px-4 pb-3 pt-4">
        <h1 className="text-[17px] font-semibold tracking-tight">Find</h1>
      </header>
      <form
        role="search"
        className="flex gap-2 px-4 pt-4"
        onSubmit={(event) => {
          event.preventDefault();
          const q = query.trim();
          if (q) void navigate({ to: "/search", search: { q } });
        }}
      >
        <label htmlFor={inputId} className="sr-only">
          Search all mail
        </label>
        <Input
          id={inputId}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search all mail"
          className="min-w-0 flex-1"
        />
        <Button type="submit" size="sm" aria-label="Search">
          <Search className="size-4" />
        </Button>
      </form>
      <ul className="mt-4 grid gap-1 px-2">
        <FindLink to="/archive" Icon={FolderArchive} label="Archive">
          Receipts, orders, bookings and documents.
        </FindLink>
        <FindLink to="/m/inbox" Icon={Inbox} label="Inbox">
          Everything, newest first.
        </FindLink>
      </ul>
    </section>
  );
}

function FindLink({
  to,
  Icon,
  label,
  children,
}: {
  to: string;
  Icon: typeof Inbox;
  label: string;
  children: string;
}) {
  return (
    <li>
      <Link to={to} className="flex items-start gap-3 rounded-md px-3 py-2.5 hover:bg-accent">
        <Icon aria-hidden className="mt-0.5 size-4 text-muted-foreground" />
        <span className="min-w-0">
          <span className="block text-[14px] text-foreground">{label}</span>
          <span className="block text-[12.5px] text-muted-foreground">{children}</span>
        </span>
      </Link>
    </li>
  );
}
