/*
 * The assistant as a copy desk: talk to your mail. It reads and searches
 * freely; every change it wants to make waits as a proof for your
 * signature, and every question waits here too. Turns run in the daemon, so
 * closing the sidebar or reloading the page doesn't stop one.
 * Anatomy and voice: docs/blueprint/29-assistant-design.md.
 */

import { Link } from "@tanstack/react-router";
import { ChevronDown, CircleDot, History as HistoryIcon, SquarePen, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { assistantToast } from "./assistantToast";

import { ASSISTANT_SHORTCUT } from "./actions";
import type { AssistantConversation } from "./api";
import { useAssistantUi } from "./assistantStore";
import { Composer } from "./Composer";
import { EmptyState } from "./EmptyState";
import { History } from "./History";
import { LiveLine } from "./LiveLine";
import { PendingCards } from "./PendingCards";
import { Transcript } from "./Transcript";
import {
  useAssistantActions,
  useAvailability,
  useConversation,
  useLandChanges,
  useLiveTurn,
  useViewContext,
} from "./useAssistant";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatChord } from "@/lib/keys/chord";
import { cn } from "@/lib/utils";

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
  const [history, setHistory] = useState(false);

  // A conversation deleted elsewhere: start fresh rather than show an error.
  useEffect(() => {
    if (conversation.isError && conversationId) showConversation(null);
  }, [conversation.isError, conversationId, showConversation]);

  const data = conversation.data;
  useLandChanges(data);
  // The live line follows the conversation's own status, so it can't outlive the turn.
  const running = data?.status === "running" || send.isPending;
  const pending = data?.status === "awaiting_user" ? data.pending : [];
  const available = availability.data?.available ?? true;
  const model = availability.data?.model ?? null;

  const submit = (text: string) => {
    const message = text.trim();
    if (!message || running) return;
    setHistory(false);
    send.mutate(
      { conversationId, message, context: viewContext() },
      { onError: (error) => assistantToast.error(errorText(error)) },
    );
  };

  return (
    <section aria-label="Assistant" className="flex h-full min-h-0 w-full flex-col bg-ink-desk">
      <header className="flex h-(--shell-topbar-h) shrink-0 items-center gap-1 border-b border-rule bg-ink-room pr-2 pl-4">
        <CircleDot className="size-4 shrink-0 text-text-2" strokeWidth={1.75} aria-hidden="true" />
        <h2 className="type-body ml-1.5 min-w-0 truncate font-medium text-text-1">
          {data?.title ?? "Assistant"}
        </h2>
        <TrustMenu
          conversation={data}
          onChange={(enabled) =>
            data &&
            trust.mutate(
              { id: data.id, enabled },
              { onError: (error) => assistantToast.error(errorText(error)) },
            )
          }
        />
        <span className="flex-1" />
        <HeaderButton
          label="Past conversations"
          pressed={history}
          onClick={() => setHistory((value) => !value)}
        >
          <HistoryIcon className="size-4" strokeWidth={1.75} />
        </HeaderButton>
        <HeaderButton
          label="New conversation"
          onClick={() => {
            setHistory(false);
            newConversation();
          }}
        >
          <SquarePen className="size-4" strokeWidth={1.75} />
        </HeaderButton>
        <HeaderButton
          label="Close assistant"
          title={`Close (${formatChord(ASSISTANT_SHORTCUT)})`}
          onClick={() => setOpen(false)}
        >
          <X className="size-4" strokeWidth={1.75} />
        </HeaderButton>
      </header>

      <ScrollToEnd
        watch={[data, live.writing]}
        className="min-h-0 flex-1 overflow-y-auto px-4 py-5"
      >
        {history ? (
          <History
            currentId={conversationId}
            onPick={showConversation}
            onClose={() => setHistory(false)}
          />
        ) : data && data.items.length > 0 ? (
          <div className="space-y-5">
            <Transcript
              key={data.id}
              items={data.items}
              running={running}
              conversationId={data.id}
              writing={live.writing}
              provenance={{ model, leavesMachine: availability.data?.leaves_machine ?? false }}
            />
            <Outcome conversation={data} onContinue={() => submit("Go on.")} />
          </div>
        ) : conversationId && conversation.isPending ? (
          <p className="type-meta text-text-3">Loading the conversation…</p>
        ) : (
          <EmptyState disabled={!available || running} onPick={submit} />
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
                  { onError: (error) => assistantToast.error(errorText(error)) },
                )
              }
            />
          </div>
        ) : null}
        {running ? (
          <LiveLine
            progress={live.progress}
            model={model}
            startedAt={live.startedAt}
            stopping={stop.isPending}
            onStop={() => data && stop.mutate(data.id)}
          />
        ) : null}
        {!available && availability.data?.reason ? (
          <p className="type-body text-text-2">
            {availability.data.reason}{" "}
            <Link
              to="/settings/$section"
              params={{ section: "llm" }}
              className="text-signal hover:underline"
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

function HeaderButton({
  label,
  title,
  pressed,
  onClick,
  children,
}: {
  label: string;
  title?: string;
  pressed?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Button
      size="icon-sm"
      variant="ghost"
      onClick={onClick}
      aria-label={label}
      aria-pressed={pressed}
      title={title ?? label}
      className={cn(
        "text-text-2 hover:bg-ink-lamp hover:text-text-1",
        pressed && "bg-ink-lamp text-text-1",
      )}
    >
      {children}
    </Button>
  );
}

/**
 * Trust, written out. A conversation that runs reversible changes without
 * asking says "trusted" in the header (a mode that changes who is
 * responsible must be visible, docs/blueprint/29), and the menu says what
 * that covers and turns it off.
 */
function TrustMenu({
  conversation,
  onChange,
}: {
  conversation: AssistantConversation | undefined;
  onChange: (enabled: boolean) => void;
}) {
  const trusted = conversation?.auto_approve ?? false;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={!conversation}
        className={cn(
          "type-meta ml-2 inline-flex shrink-0 items-center gap-0.5 outline-none hover:text-text-1 focus-visible:text-text-1 disabled:opacity-50",
          trusted ? "text-signal" : "text-text-3",
        )}
      >
        · {trusted ? "trusted" : "asks first"}
        <ChevronDown className="size-3" strokeWidth={1.75} aria-hidden="true" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72">
        <p className="type-body px-2 py-1.5 text-text-2">
          {trusted
            ? "Archive, read, star and moves run without asking in this conversation, and say so. Sending, deleting, trash, spam and rules still ask."
            : "Every change waits for your signature in this conversation."}
        </p>
        <DropdownMenuItem onSelect={() => onChange(!trusted)}>
          {trusted
            ? "Turn off: ask for every change"
            : "Trust this conversation with reversible changes"}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function Outcome({
  conversation,
  onContinue,
}: {
  conversation: AssistantConversation;
  onContinue: () => void;
}) {
  const [details, setDetails] = useState(false);
  if (conversation.status !== "idle") return null;
  switch (conversation.last_outcome) {
    case "failed":
      return (
        <div role="alert" className="border-l border-danger pl-3">
          <p className="type-body text-danger">
            {conversation.last_error ?? "That turn didn't finish. Send your message again."}
          </p>
          {conversation.last_error_detail ? (
            <>
              <button
                type="button"
                aria-expanded={details}
                onClick={() => setDetails((value) => !value)}
                className="type-meta mt-1 text-text-3 outline-none hover:text-text-1 focus-visible:text-text-1"
              >
                {details ? "▾" : "▸"} Details
              </button>
              {details ? (
                <pre className="type-meta mt-1 max-h-40 overflow-auto bg-ink-room p-2 break-words whitespace-pre-wrap text-text-2">
                  {conversation.last_error_detail}
                </pre>
              ) : null}
            </>
          ) : null}
        </div>
      );
    case "step_limit":
      return (
        <div className="type-body flex items-center gap-2 text-text-2">
          <span>I stopped after many steps.</span>
          <Button size="xs" variant="outline" onClick={onContinue}>
            Keep going
          </Button>
        </div>
      );
    case "cancelled":
      return <p className="type-meta text-text-3">Stopped.</p>;
    case "interrupted":
      return (
        <p className="type-meta text-text-3">
          mxr restarted during this turn. Send a message to carry on.
        </p>
      );
    default:
      return null;
  }
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

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
