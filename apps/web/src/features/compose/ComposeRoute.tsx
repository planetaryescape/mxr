import { useRouterState } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useMemo } from "react";

import { EmptyState } from "@/components/EmptyState";
import { Button } from "@/components/ui/button";
import type { ComposeKind } from "./api";
import { ComposeEditorPanel } from "./ComposeEditorPanel";
import { useComposeSession, type ComposeIntent } from "./useComposeSession";

type ComposeSearch = Record<string, unknown>;

export function ComposeRoute() {
  const location = useRouterState({ select: (state) => state.location });
  const intent = useMemo(
    () => composeIntent(location.pathname, location.search as ComposeSearch),
    [location.pathname, location.search],
  );

  const controller = useComposeSession(intent);

  if (controller.sessionLoading) {
    return <ComposeLoading title={intent.title} />;
  }

  if (controller.sessionError) {
    return (
      <EmptyState
        icon={RefreshCw}
        title="Compose unavailable"
        description={controller.sessionError.message}
        action={<Button onClick={controller.retrySession}>Retry</Button>}
      />
    );
  }

  if (!controller.draft) return null;

  return <ComposeEditorPanel controller={controller} />;
}

function ComposeLoading({ title }: { title: string }) {
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <div className="h-14 shrink-0 border-b border-border" />
      <div className="shrink-0 border-b border-border px-5 py-3">
        <div className="mx-auto w-full max-w-[860px] space-y-2">
          <div className="h-8 animate-pulse rounded-md bg-muted" />
          <div className="h-8 w-2/3 animate-pulse rounded-md bg-muted/70" />
        </div>
      </div>
      <div className="min-h-0 flex-1 p-5">
        <div className="mx-auto h-full w-full max-w-[860px] animate-pulse rounded-md bg-muted/40" />
      </div>
      <div className="shrink-0 border-t border-border px-5 py-3 font-mono text-2xs text-muted-foreground">
        Opening {title.toLowerCase()}…
      </div>
    </div>
  );
}

function composeIntent(pathname: string, search: ComposeSearch): ComposeIntent {
  const draftMatch = pathname.match(/^\/compose\/([^/]+)$/);
  const draftId = draftMatch?.[1] ? decodeURIComponent(draftMatch[1]) : undefined;
  if (draftId && draftId !== "new") {
    return { key: `draft:${draftId}`, title: "Saved draft", kind: "new", draftId };
  }
  const reply = typeof search.reply === "string" ? search.reply : undefined;
  const prefillTo = typeof search.to === "string" ? search.to : undefined;
  const prefillSubject = typeof search.subject === "string" ? search.subject : undefined;
  const mode =
    search.mode === "forward" || search.mode === "all" || search.mode === "single"
      ? search.mode
      : undefined;
  const kind: ComposeKind = reply
    ? mode === "forward"
      ? "forward"
      : mode === "all"
        ? "reply_all"
        : "reply"
    : "new";
  const title =
    kind === "forward"
      ? "Forward message"
      : kind === "reply_all"
        ? "Reply all"
        : kind === "reply"
          ? "Reply"
          : "New message";
  const prefillKey = [prefillTo?.trim() ?? "", prefillSubject?.trim() ?? ""].join("|");
  const composeKey = reply ?? (prefillKey || "new");
  return {
    key: `compose:${kind}:${composeKey}`,
    title,
    kind,
    messageId: reply,
    prefillTo,
    prefillSubject,
  };
}
