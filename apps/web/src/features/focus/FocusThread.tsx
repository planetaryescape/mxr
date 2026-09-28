/*
 * The left side of focus mode: what this conversation needs from you
 * (the reader's context block: gist, ask, how you know them, promises) and
 * the latest message, with earlier ones folded. The same components as the
 * reader, so the ask is highlighted in the same place.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { llmPolicyKey, useLlmStatus } from "@/features/llm/useLlmStatus";
import { fetchThread, resolveCommitment } from "@/features/mailbox/api";
import {
  fetchThreadGist,
  threadContextKey,
  threadContextQuery,
  threadGistKey,
  threadGistQuery,
} from "@/features/thread/context/api";
import { ASK_MARK_ATTRIBUTE } from "@/features/thread/context/askQuote";
import { ContextBlock } from "@/features/thread/context/ContextBlock";
import { HeadersDialog } from "@/features/thread/HeadersDialog";
import { MessageCard } from "@/features/thread/MessageCard";
import { ReaderSkeleton } from "@/features/thread/ReaderSkeleton";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { plural } from "@/lib/format";
import { useUiPrefs, type ReaderView } from "@/state/uiPrefsStore";

/** Earlier messages shown folded above the newest; the rest are counted. */
const EARLIER_SHOWN = 1;

export function FocusThread({ threadId }: { threadId: string }) {
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
  });
  const context = useQuery(threadContextQuery(threadId));
  const llm = useLlmStatus();
  const policy = llmPolicyKey(llm.data?.status);
  const gist = useQuery({ ...threadGistQuery(threadId, policy), enabled: llm.enabled });
  const queryClient = useQueryClient();
  const defaultView = useUiPrefs((s) => s.readerView);
  const [view, setView] = useState<ReaderView>(defaultView);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [remoteAllowed, setRemoteAllowed] = useState(false);
  const [headersFor, setHeadersFor] = useState<string | null>(null);
  const phase = useDelayedPending(thread.isLoading);
  const resolve = useMutation({
    mutationFn: resolveCommitment,
    onSuccess: () => {
      toast.success("Promise marked done");
      void queryClient.invalidateQueries({ queryKey: threadContextKey(threadId) });
    },
    onError: (error) => toast.error("Couldn't mark it done", { description: error.message }),
  });

  if (phase !== "ready") return <ReaderSkeleton quiet={phase === "quiet"} />;
  if (thread.isError || !thread.data) {
    return (
      <p className="px-5 py-6 text-sm text-muted-foreground">
        Couldn't open this conversation: {thread.error?.message ?? "not found"}.
      </p>
    );
  }
  const data = thread.data;
  const bodies = new Map(data.bodies.map((body) => [body.message_id, body]));
  const newest = data.messages.at(-1);
  const shown = data.messages.slice(-(EARLIER_SHOWN + 1));
  const folded = data.messages.length - shown.length;
  const askQuote = gist.data?.status === "ready" ? (gist.data.ask?.quote ?? null) : null;
  const toggle = (id: string) =>
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div className="@container pb-10" data-focus-thread>
      <ContextBlock
        context={context.data}
        gist={{
          reserved: llm.enabled,
          data: gist.data,
          loading: gist.isFetching && !gist.data,
          error: gist.error?.message ?? null,
          retry: () =>
            void queryClient.fetchQuery({
              queryKey: threadGistKey(threadId, policy),
              queryFn: () => fetchThreadGist(threadId, true),
              staleTime: 0,
            }),
        }}
        onRevealAsk={() =>
          document
            .querySelector(`[data-focus-thread] [${ASK_MARK_ATTRIBUTE}]`)
            ?.scrollIntoView({ block: "center", behavior: "smooth" })
        }
        onResolvePromise={(id) => resolve.mutate(id)}
        resolving={resolve.isPending}
      />
      {folded > 0 ? (
        <p className="px-5 pt-3 font-mono text-2xs text-muted-foreground">
          {plural(folded, "earlier message")} in this conversation
        </p>
      ) : null}
      {shown.map((message) => (
        <MessageCard
          key={message.id}
          message={message}
          body={bodies.get(message.id)}
          expanded={
            message.id === newest?.id ? !expanded.has(message.id) : expanded.has(message.id)
          }
          focused={false}
          view={view}
          showQuotes={false}
          showSignature={false}
          remoteAllowedForThread={remoteAllowed}
          askQuote={askQuote?.message_id === message.id ? askQuote.text : undefined}
          onToggle={() => toggle(message.id)}
          onAllowRemote={() => setRemoteAllowed(true)}
          onShowFormatted={() => setView("formatted")}
          onShowHeaders={() => setHeadersFor(message.id)}
        />
      ))}
      {headersFor ? (
        <HeadersDialog messageId={headersFor} onClose={() => setHeadersFor(null)} />
      ) : null}
    </div>
  );
}
