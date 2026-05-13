import { useRouterState } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useEffect } from "react";

import { MailboxList } from "./MailboxList";
import { SyncProgressBanner } from "./SyncProgressBanner";
import { useMailboxQuery } from "./useMailboxQuery";
import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import { useMailboxPane } from "@/state/mailboxPaneStore";

export function MailboxRoute() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const routeParts = pathname.split("/").filter(Boolean);
  const activeThreadId =
    routeParts[0] === "m" && routeParts[1] && routeParts[2] && routeParts[1] !== "label"
      ? routeParts[2]
      : undefined;
  const mailboxPath = activeThreadId ? `/m/${routeParts[1]}` : pathname;
  const activePane = useMailboxPane((state) => state.activePane);
  const setActivePane = useMailboxPane((state) => state.setActivePane);
  const mailbox = useMailboxQuery();

  useEffect(() => {
    if (!activeThreadId && activePane === "reader") setActivePane("mailbox");
  }, [activePane, activeThreadId, setActivePane]);

  if (mailbox.isLoading) {
    return (
      <div className="flex flex-1 flex-col border-r border-border">
        <MailboxHeader title="Loading" subtitle="Opening local mailbox" />
        <div className="space-y-0 p-3">
          {Array.from({ length: 12 }, (_, index) => (
            <div key={index} className="mb-2 h-12 animate-pulse rounded-md bg-muted/60" />
          ))}
        </div>
      </div>
    );
  }

  if (mailbox.isError) {
    return (
      <EmptyState
        icon={RefreshCw}
        title="Mailbox unavailable"
        description={mailbox.error.message}
        action={<Button onClick={() => mailbox.refetch()}>Retry</Button>}
      />
    );
  }

  const data = mailbox.data;
  if (!data) return null;
  return (
    <div className="flex min-w-0 flex-1 flex-col border-r border-border bg-background">
      <SyncProgressBanner />
      <MailboxHeader
        title={data.mailbox.lensLabel}
        subtitle={`${data.mailbox.counts.unread ?? 0} unread / ${data.mailbox.counts.total ?? 0} total`}
      />
      <MailboxList
        groups={data.mailbox.groups}
        mailboxPath={mailboxPath}
        activeThreadId={activeThreadId}
        previewOnFocus={Boolean(activeThreadId)}
        hasMore={mailbox.hasNextPage}
        loadingMore={mailbox.isFetchingNextPage}
        onLoadMore={() => {
          void mailbox.fetchNextPage();
        }}
      />
    </div>
  );
}

function MailboxHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <div className="flex h-12 items-center justify-between border-b border-border px-4">
      <div>
        <h1 className="text-sm font-semibold tracking-tight">{title}</h1>
        <div className="font-mono text-2xs text-muted-foreground">{subtitle}</div>
      </div>
    </div>
  );
}
