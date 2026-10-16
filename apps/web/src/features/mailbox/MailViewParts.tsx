/*
 * Shared list states: the loading skeleton and the centred empty/error
 * message used by every list page.
 */

import type { ReactNode } from "react";

import { AvatarSkeleton } from "@/components/Avatar";

/**
 * `quiet`: the first 300 ms of a load show the same frame, empty (see
 * `useDelayedPending`), so a fast load never flashes a skeleton.
 */
export function ListSkeleton({ quiet = false }: { quiet?: boolean }) {
  return (
    <div
      className="flex-1 overflow-hidden"
      aria-busy="true"
      aria-label="Loading conversations"
      data-testid={quiet ? undefined : "list-skeleton"}
    >
      {Array.from({ length: quiet ? 0 : 12 }, (_, index) => (
        <div key={index} className="flex items-center gap-3 border-b border-border/60 px-3 py-3">
          <AvatarSkeleton />
          <div className="grid flex-1 gap-1.5">
            <div
              className="h-3 w-1/3 animate-pulse rounded-sm bg-muted"
              style={{ animationDelay: `${index * 40}ms` }}
            />
            <div
              className="h-3 w-3/4 animate-pulse rounded-sm bg-muted/70"
              style={{ animationDelay: `${index * 40}ms` }}
            />
          </div>
          <div className="h-3 w-12 animate-pulse rounded-sm bg-muted/60" />
        </div>
      ))}
    </div>
  );
}

export function Centered({
  icon,
  title,
  body,
  action,
}: {
  icon: ReactNode;
  title: string;
  body?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-8 py-16 text-center">
      <div className="mb-1 text-muted-foreground">{icon}</div>
      <h2 className="text-md font-semibold">{title}</h2>
      {body ? <p className="max-w-sm text-sm text-muted-foreground">{body}</p> : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}
