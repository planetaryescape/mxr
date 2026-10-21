import { Link } from "@tanstack/react-router";
import type { LucideIcon } from "lucide-react";

import { MODE_GLYPHS } from "@/components/ModeMark";
import { PageFrame } from "@/components/PageFrame";

/**
 * Find, a short list of three places (Archive, Updates, Inbox). The phone's
 * More sheet lists them too; search is the top bar's one field.
 */
export function FindRoute() {
  return (
    <section aria-label="Find" className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
      <PageFrame title="Find" purpose="Go straight to a place. Search is in the top bar." />
      <ul className="mt-4 grid gap-1 px-2">
        <FindLink to="/archive" Icon={MODE_GLYPHS.archive} label="Archive">
          Receipts, orders, bookings and documents.
        </FindLink>
        <FindLink to="/updates" Icon={MODE_GLYPHS.updates} label="Updates">
          Notifications and receipts from services, by sender.
        </FindLink>
        <FindLink to="/m/inbox" Icon={MODE_GLYPHS.inbox} label="Inbox">
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
  Icon: LucideIcon;
  label: string;
  children: string;
}) {
  return (
    <li>
      <Link to={to} className="flex items-start gap-3 rounded-md px-3 py-2.5 hover:bg-accent">
        <Icon aria-hidden className="mt-0.5 size-4 text-muted-foreground" />
        <span className="min-w-0">
          <span className="block text-base text-foreground">{label}</span>
          <span className="block text-xs text-muted-foreground">{children}</span>
        </span>
      </Link>
    </li>
  );
}
