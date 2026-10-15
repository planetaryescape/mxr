/*
 * The assistant sidebar: talk to your mail. It reads and searches freely;
 * every change it wants to make, and every question it needs answered,
 * waits here for you. Turns run in the daemon, so closing the sidebar or
 * reloading the page doesn't stop one.
 */

import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  ArrowUp,
  BotMessageSquare,
  History,
  LoaderCircle,
  MailQuestion,
  Newspaper,
  ShieldCheck,
  Sparkles,
  Square,
  SquarePen,
  Trash2,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { toast } from "sonner";

import { assistantKeys, deleteConversation, type AssistantConversation } from "./api";
import { useAssistantUi } from "./assistantStore";
import { toolWords } from "./describe";
import { PendingCards } from "./PendingCards";
import { Transcript } from "./Transcript";
import {
  useAssistantActions,
  useAvailability,
  useConversation,
  useConversations,
  useLiveTurn,
  useViewContext,
} from "./useAssistant";
import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Textarea } from "@/components/ui/textarea";
import { formatChord } from "@/lib/keys/chord";
import { formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";

export const ASSISTANT_SHORTCUT = "Mod+.";

const SUGGESTIONS: Array<{ text: string; icon: LucideIcon }> = [
  { text: "What needs a reply from me this week?", icon: MailQuestion },
  { text: "Summarise what arrived today", icon: Sparkles },
  { text: "Find newsletters I never open and tidy them up", icon: Newspaper },
];

export function AssistantSidebar() {
  const conversationId = useAssistantUi((s) => s.conversationId);
  const showConversation = useAssistantUi((s) => s.showConversation);
  const newConversation = useAssistantUi((s) => s.newConversation);
  const setOpen = useAssistantUi((s) => s.setOpen);
  const availability = useAvailability();
  const conversation = useConversation(conversationId);
  const live = useLiveTurn(conversationId);
  const { send, answer, stop, trust } = useAssistantActions();
  const viewContext = useViewContext();

  // A conversation deleted elsewhere: start fresh rather than show an error.
  useEffect(() => {
    if (conversation.isError && conversationId) showConversation(null);
  }, [conversation.isError, conversationId, showConversation]);

  const data = conversation.data;
  const running = data?.status === "running" || send.isPending;
  const pending = data?.status === "awaiting_user" ? data.pending : [];
  const available = availability.data?.available ?? true;

  const submit = (text: string) => {
    const message = text.trim();
    if (!message || running) return;
    send.mutate(
      { conversationId, message, context: viewContext() },
      { onError: (error) => toast.error(errorText(error)) },
    );
  };

  return (
    <section aria-label="Assistant" className="flex h-full min-h-0 w-full flex-col bg-background">
      <header className="flex h-[var(--shell-topbar-h)] shrink-0 items-center gap-1 border-b border-border pr-2 pl-4">
        <BotMessageSquare className="size-4 shrink-0 text-primary" aria-hidden="true" />
        <h2 className="ml-1.5 min-w-0 flex-1 truncate text-sm font-medium text-foreground">
          {data?.title ?? "Assistant"}
        </h2>
        <Button
          size="icon-sm"
          variant="ghost"
          disabled={!data}
          aria-pressed={data?.auto_approve ?? false}
          className={cn(
            data?.auto_approve
              ? "bg-primary/10 text-primary hover:bg-primary/15"
              : "text-muted-foreground",
          )}
          onClick={() =>
            data &&
            trust.mutate(
              { id: data.id, enabled: !data.auto_approve },
              { onError: (error) => toast.error(errorText(error)) },
            )
          }
          aria-label="Approve reversible changes automatically"
          title={
            data
              ? data.auto_approve
                ? "Reversible changes run without asking. Click to ask for every change again."
                : "Let reversible changes (archive, read, star, moves) run without asking in this conversation"
              : "Start a conversation to choose what runs without asking"
          }
        >
          <ShieldCheck className="size-4" />
        </Button>
        <HistoryMenu currentId={conversationId} onPick={showConversation} />
        <Button
          size="icon-sm"
          variant="ghost"
          onClick={newConversation}
          aria-label="New conversation"
          title="New conversation"
        >
          <SquarePen className="size-4" />
        </Button>
        <Button
          size="icon-sm"
          variant="ghost"
          onClick={() => setOpen(false)}
          aria-label="Close assistant"
          title={`Close (${formatChord(ASSISTANT_SHORTCUT)})`}
        >
          <X className="size-4" />
        </Button>
      </header>

      {data?.auto_approve ? (
        <p className="flex shrink-0 items-start gap-2 border-b border-border bg-primary/5 px-4 py-2 text-2xs text-muted-foreground">
          <ShieldCheck className="mt-px size-3.5 shrink-0 text-primary" aria-hidden="true" />
          <span>
            <span className="text-foreground">Reversible changes run without asking.</span> Sending,
            deleting, trash, spam and rules still ask.
          </span>
        </p>
      ) : null}
      <ScrollToEnd
        watch={[data, live.writing]}
        className="min-h-0 flex-1 overflow-y-auto px-4 py-5"
      >
        {data && data.items.length > 0 ? (
          <div className="space-y-5">
            <Transcript items={data.items} running={running} writing={live.writing} />
            <Outcome conversation={data} onContinue={() => submit("Go on.")} />
          </div>
        ) : conversationId && conversation.isPending ? (
          <p className="text-xs text-muted-foreground">Loading the conversation…</p>
        ) : (
          <EmptyState
            disabled={!available || running}
            onPick={(suggestion) => submit(suggestion)}
          />
        )}
      </ScrollToEnd>

      <div className="shrink-0 space-y-2.5 px-3 pt-1 pb-3">
        {pending.length > 0 && data ? (
          <div className="max-h-[55vh] overflow-y-auto">
            <PendingCards
              pending={pending}
              busy={answer.isPending}
              onDecide={(decision) =>
                answer.mutate(
                  { id: data.id, decisions: [decision] },
                  { onError: (error) => toast.error(errorText(error)) },
                )
              }
            />
          </div>
        ) : null}
        {running ? (
          <RunningLine
            progress={live.writing ? { kind: "writing", text: live.writing } : live.progress}
            startedAt={live.startedAt}
            stopping={stop.isPending}
            onStop={() => data && stop.mutate(data.id)}
          />
        ) : null}
        {!available && availability.data?.reason ? (
          <p className="text-xs text-muted-foreground">
            {availability.data.reason}{" "}
            <Link
              to="/settings/$section"
              params={{ section: "llm" }}
              className="text-primary hover:underline"
            >
              AI settings
            </Link>
          </p>
        ) : null}
        <Composer
          disabled={!available}
          running={running}
          waiting={pending.length > 0}
          onSend={submit}
        />
      </div>
    </section>
  );
}

function Composer({
  disabled,
  running,
  waiting,
  onSend,
}: {
  disabled: boolean;
  running: boolean;
  waiting: boolean;
  onSend: (text: string) => void;
}) {
  const [text, setText] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const focusRequest = useAssistantUi((s) => s.focusRequest);

  useEffect(() => {
    if (focusRequest === 0) return;
    // After the pane's open transition has placed it.
    const frame = requestAnimationFrame(() => input.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [focusRequest]);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (!text.trim() || running || disabled) return;
    onSend(text);
    setText("");
  };

  return (
    <form
      onSubmit={submit}
      className={cn(
        "rounded-lg border border-border bg-surface shadow-sm shadow-black/10",
        "focus-within:border-primary/60 focus-within:ring-2 focus-within:ring-ring/20",
        disabled && "opacity-60",
      )}
    >
      <Textarea
        ref={input}
        aria-label="Message the assistant"
        value={text}
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          // Enter sends; Shift+Enter adds a line.
          if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault();
            submit();
          }
          if (event.key === "Escape") input.current?.blur();
        }}
        placeholder={
          waiting ? "Answer above, or write to move on" : "Ask about your mail, or say what to do"
        }
        rows={2}
        className="max-h-48 min-h-14 resize-none border-0 bg-transparent px-3 pt-2.5 pb-1 text-sm shadow-none focus-visible:ring-0 focus-visible:ring-offset-0"
      />
      <div className="flex items-center gap-2 pr-1.5 pb-1.5 pl-3">
        <span className="min-w-0 flex-1 truncate text-2xs text-muted-foreground">
          {waiting && text.trim() ? "Sending this declines what is waiting above." : <Disclosure />}
        </span>
        <Button
          type="submit"
          size="icon-xs"
          className="size-7 rounded-md"
          disabled={!text.trim() || running || disabled}
          aria-label="Send"
          title="Send (Enter)"
        >
          <ArrowUp className="size-4" />
        </Button>
      </div>
    </form>
  );
}

function RunningLine({
  progress,
  startedAt,
  stopping,
  onStop,
}: {
  progress: ReturnType<typeof useLiveTurn>["progress"];
  startedAt: number | null;
  stopping: boolean;
  onStop: () => void;
}) {
  const elapsed = useElapsed(startedAt);
  const doing =
    progress?.kind === "calling"
      ? progress.reason || toolWords(progress.tool)
      : progress?.kind === "summarizing"
        ? "Summarising earlier messages to make room"
        : progress?.kind === "writing"
          ? "Writing"
          : "Thinking";
  return (
    <div className="flex items-center gap-2 px-1" role="status" aria-live="polite">
      <LoaderCircle className="size-3.5 shrink-0 animate-spin text-primary" aria-hidden="true" />
      <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{doing}</span>
      {elapsed !== null ? (
        <span className="font-mono text-2xs tabular-nums text-muted-foreground">{elapsed}s</span>
      ) : null}
      <Button size="xs" variant="ghost" onClick={onStop} disabled={stopping}>
        <Square className="size-3" />
        Stop
      </Button>
    </div>
  );
}

function Outcome({
  conversation,
  onContinue,
}: {
  conversation: AssistantConversation;
  onContinue: () => void;
}) {
  if (conversation.status !== "idle") return null;
  switch (conversation.last_outcome) {
    case "failed":
      return (
        <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          That turn failed: {conversation.last_error ?? "unknown error"}. Send a message to try
          again.
        </p>
      );
    case "step_limit":
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>I stopped after many steps.</span>
          <Button size="xs" variant="outline" onClick={onContinue}>
            Keep going
          </Button>
        </div>
      );
    case "cancelled":
      return <p className="font-mono text-2xs text-muted-foreground">Stopped.</p>;
    case "interrupted":
      return (
        <p className="font-mono text-2xs text-muted-foreground">
          mxr restarted during this turn. Send a message to carry on.
        </p>
      );
    default:
      return null;
  }
}

function EmptyState({ disabled, onPick }: { disabled: boolean; onPick: (text: string) => void }) {
  return (
    <div className="flex min-h-full flex-col justify-end gap-6 pb-2">
      <div className="space-y-3">
        <span className="inline-flex size-9 items-center justify-center rounded-lg border border-border bg-surface text-primary">
          <BotMessageSquare className="size-5" aria-hidden="true" />
        </span>
        <div className="space-y-1">
          <p className="text-base font-medium text-foreground">Talk to your mail.</p>
          <p className="text-xs leading-relaxed text-muted-foreground">
            Ask questions, get summaries, or have it sort, archive and draft. It reads freely; every
            change waits for your approval.
          </p>
        </div>
      </div>
      <div className="space-y-1.5">
        <p className="font-mono text-2xs tracking-wider text-muted-foreground uppercase">Try</p>
        <ul className="-mx-2">
          {SUGGESTIONS.map(({ text, icon: Icon }) => (
            <li key={text}>
              <button
                type="button"
                disabled={disabled}
                onClick={() => onPick(text)}
                className="group flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left text-sm text-foreground/90 outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
              >
                <Icon
                  className="size-4 shrink-0 text-muted-foreground group-hover:text-primary"
                  aria-hidden="true"
                />
                {text}
              </button>
            </li>
          ))}
        </ul>
      </div>
      <p className="text-2xs text-muted-foreground">
        <KeyChip>{formatChord(ASSISTANT_SHORTCUT)}</KeyChip> opens and closes this panel.
      </p>
    </div>
  );
}

/** Where the conversation goes, said once, inside the composer. */
function Disclosure() {
  const availability = useAvailability();
  const data = availability.data;
  if (!data?.available || !data.model) return null;
  return (
    <span
      className="font-mono"
      title={
        data.leaves_machine
          ? "Your messages and the mail it reads go to this provider"
          : "Nothing leaves this machine"
      }
    >
      {data.model} · {data.leaves_machine ? "sent off this machine" : "runs on this machine"}
    </span>
  );
}

function HistoryMenu({
  currentId,
  onPick,
}: {
  currentId: string | null;
  onPick: (id: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const conversations = useConversations(open);
  const queryClient = useQueryClient();
  const remove = async (id: string) => {
    try {
      await deleteConversation(id);
      if (id === currentId) onPick(null);
      void queryClient.invalidateQueries({ queryKey: assistantKeys.list });
      queryClient.removeQueries({ queryKey: assistantKeys.conversation(id) });
    } catch (error) {
      toast.error(errorText(error));
    }
  };
  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label="Past conversations"
          title="Past conversations"
        >
          <History className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80">
        <DropdownMenuLabel>Conversations</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {conversations.isPending ? (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">Loading…</p>
        ) : (conversations.data ?? []).length === 0 ? (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">None yet.</p>
        ) : (
          (conversations.data ?? []).map((summary) => (
            <DropdownMenuItem
              key={summary.id}
              onSelect={() => onPick(summary.id)}
              className={cn("group gap-2", summary.id === currentId && "bg-muted/60")}
            >
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm">{summary.title}</span>
                <span className="block font-mono text-2xs text-muted-foreground">
                  {formatRelative(new Date(summary.updated_at * 1000))}
                  {summary.pending_count > 0 ? " · waiting for you" : ""}
                  {summary.status === "running" ? " · working" : ""}
                </span>
              </span>
              <button
                type="button"
                aria-label={`Delete "${summary.title}"`}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  void remove(summary.id);
                }}
                className="rounded-sm p-1 text-muted-foreground opacity-0 group-hover:opacity-100 group-data-[highlighted]:opacity-100 hover:text-destructive focus-visible:opacity-100"
              >
                <Trash2 className="size-3.5" />
              </button>
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Keeps the newest message in view while the reader is at the bottom. */
function ScrollToEnd({
  watch,
  className,
  children,
}: {
  watch: unknown;
  className?: string;
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useLayoutEffect(() => {
    const element = ref.current;
    if (element && pinned.current) element.scrollTop = element.scrollHeight;
  }, [watch]);
  return (
    <div
      ref={ref}
      className={className}
      onScroll={(event) => {
        const element = event.currentTarget;
        pinned.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48;
      }}
    >
      {children}
    </div>
  );
}

function useElapsed(startedAt: number | null): number | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (startedAt === null) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [startedAt]);
  return startedAt === null ? null : Math.max(0, Math.round((now - startedAt) / 1000));
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
