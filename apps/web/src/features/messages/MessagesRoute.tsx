import { useNavigate } from "@tanstack/react-router";
import { ChevronRight, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import {
  prefilledMessageIntent,
  replyIntent,
  useComposeUi,
} from "@/features/compose/composeUiStore";
import { onSendEvent } from "@/features/compose/session/sendEvents";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { ensureThread, targetFromThread } from "@/features/mail-actions/target";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useModeGuide, useRetireCard, type ModeGuide } from "@/features/modes/api";
import { useThreadModesMap } from "@/features/modes/membership";
import { ModeCard } from "@/features/modes/ModeCard";
import { markModeDone } from "@/features/modes/modeDone";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane } from "@/hooks/useSplitPane";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { useScopeController } from "@/lib/keys/controllers";

import {
  refreshMessages,
  setPinned,
  useMessagesQuery,
  usePersonQuery,
  type MessagesData,
  type MessagesRow,
  type MessagesTurn,
  type PersonPage,
} from "./api";
import { MessagesList } from "./MessagesList";
import { cursorRows, newTopicAddress, rowForThread, stepTopic } from "./messagesView";
import { PersonPane } from "./PersonPane";
import { useGotIt } from "./useGotIt";

/** List and person page share the screen from Tailwind's md up. */
const MESSAGES_SPLIT_QUERY = "(min-width: 768px)";
const PEOPLE_PANE_SIZE = { defaultSize: "22rem", minSize: "16rem", maxSize: "40rem" };

export interface MessagesSearch {
  /** The selected row's id: `person:<email>` or `group:<thread>`. */
  person?: string;
  /** The selected topic's thread. */
  topic?: string;
  /** Only rows where it is this side's turn. */
  turn?: MessagesTurn;
}

/**
 * Messages: people you talk with, one row each, with your conversations
 * inside as topics. The daemon builds the bands, order, previews and each
 * message's new text (`ListMessages`, `GetPerson`); this page lays them out
 * as a list beside one person's page (two screens on a phone) and wires the
 * mode's verbs: reply, got it, done here.
 */
export function MessagesRoute({
  search,
  threadId,
}: {
  search: MessagesSearch;
  /** From /messages/$threadId: open the row holding this conversation. */
  threadId?: string;
}) {
  const messages = useMessagesQuery(search.turn ?? null);
  const guide = useModeGuide("messages");
  const phase = useDelayedPending(messages.isLoading);
  return (
    <section aria-label="Messages" className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
      {phase !== "ready" ? (
        <>
          <MessagesHeader guide={guide.data} turn={search.turn} />
          <ListSkeleton quiet={phase === "quiet"} />
        </>
      ) : messages.isError ? (
        <>
          <MessagesHeader guide={guide.data} turn={search.turn} />
          <Centered
            icon={<RefreshCw className="size-6" />}
            title="Couldn't load Messages"
            body={messages.error.message}
            action={
              <Button size="sm" onClick={() => void messages.refetch()}>
                Try again
              </Button>
            }
          />
        </>
      ) : messages.data ? (
        <MessagesBody data={messages.data} guide={guide.data} search={search} threadId={threadId} />
      ) : null}
    </section>
  );
}

function MessagesHeader({ guide, turn }: { guide?: ModeGuide; turn?: MessagesTurn }) {
  return (
    <header className="shrink-0 border-b border-border px-5 pb-3 pt-4">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className="text-[17px] font-semibold tracking-tight text-foreground">
          {turn === "theirs" ? "Waiting on" : "Messages"}
        </h1>
        <p data-testid="mode-header" className="min-w-0 text-[12.5px] text-muted-foreground">
          {guide?.header}
        </p>
      </div>
    </header>
  );
}

function MessagesBody({
  data,
  guide,
  search,
  threadId,
}: {
  data: MessagesData;
  guide?: ModeGuide;
  search: MessagesSearch;
  threadId?: string;
}) {
  const navigate = useNavigate();
  const [quietOpen, setQuietOpen] = useState(false);
  // A phone shows the list or the person page, never both.
  const [pageOpen, setPageOpen] = useState(Boolean(search.person || threadId));
  const [asSent, setAsSent] = useState<string | null>(null);
  const rows = useMemo(() => cursorRows(data, quietOpen), [data, quietOpen]);
  // "Also in To do: …" and a new sender's question on each shown row.
  const firstTopics = useMemo(
    () =>
      [...data.your_turn, ...data.recent]
        .flatMap((row) => (row.topics[0] ? [row.topics[0].thread_id] : []))
        .slice(0, 100),
    [data],
  );
  const memberships = useThreadModesMap(firstTopics).data;

  // The selected row: the URL's, else the row holding a linked thread, else
  // the first.
  const linked = threadId ? rowForThread(data, threadId) : undefined;
  const allRows = useMemo(
    () => [...data.your_turn, ...data.pinned, ...data.recent, ...data.quiet],
    [data],
  );
  const selectedId = search.person ?? linked?.id ?? rows[0]?.id ?? null;
  const selected: MessagesRow | undefined = allRows.find((row) => row.id === selectedId);
  const topic = search.topic ?? (linked ? threadId : undefined) ?? null;
  const person = usePersonQuery(selectedId, topic);
  const page: PersonPage | undefined = person.data;
  const conversation = page?.conversation ?? null;
  const [replyAllOverride, setReplyAllOverride] = useState<boolean | null>(null);
  const replyAll = replyAllOverride ?? conversation?.composer.reply_all ?? false;
  useEffect(() => {
    setReplyAllOverride(null);
    setAsSent(null);
  }, [conversation?.thread_id]);

  const select = useCallback(
    (id: string, options: { topic?: string; open?: boolean } = {}) => {
      void navigate({
        to: "/messages",
        search: { ...(search.turn ? { turn: search.turn } : {}), person: id, topic: options.topic },
        replace: true,
      });
      if (options.open) setPageOpen(true);
    },
    [navigate, search.turn],
  );
  const listRef = useRef<HTMLDivElement>(null);
  const split = useMediaQuery(MESSAGES_SPLIT_QUERY);
  const pane = useSplitPane("messages", PEOPLE_PANE_SIZE, { active: split });
  useEffect(() => {
    if (!selectedId) return;
    listRef.current
      ?.querySelector(`[data-row-id="${CSS.escape(selectedId)}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  const retire = useRetireCard("messages");
  const { mutate: retireMutate } = retire;
  const cardSeen = guide?.card_seen ?? true;
  const cardShown = Boolean(guide && !cardSeen && allRows.length > 0);
  const retireCard = useCallback(() => {
    if (!cardSeen) retireMutate();
  }, [cardSeen, retireMutate]);

  // After a reply or Got it, the next person whose turn it is.
  const nextYourTurn = useCallback(() => {
    const next = data.your_turn.find((row) => row.id !== selectedId);
    if (next) select(next.id);
  }, [data.your_turn, select, selectedId]);

  const gotIt = useGotIt(`${selectedId ?? ""}|${conversation?.thread_id ?? ""}`, () => {
    retireCard();
    nextYourTurn();
  });

  // A reply sent from this page's composer (its own ⌘Enter or Send) moves
  // on to the next person whose turn it is, as Focus & reply does.
  const replyKeys = useRef(new Set<string>());
  useEffect(
    () =>
      onSendEvent((event) => {
        if (!replyKeys.current.has(event.intentKey)) return;
        if (event.kind === "queued") {
          retireCard();
          nextYourTurn();
        }
        if (event.kind === "sent") void refreshMessages();
      }),
    [nextYourTurn, retireCard],
  );

  const openReply = useCallback(
    (all: boolean) => {
      if (!conversation) return;
      const intent = replyIntent(conversation.composer.reply_to_message_id, all ? "all" : "single");
      replyKeys.current.add(intent.key);
      setPageOpen(true);
      useComposeUi.getState().openCompose(intent, "inline");
    },
    [conversation],
  );

  const done = useCallback(() => {
    if (!conversation) return;
    void markModeDone("messages", [conversation.thread_id]);
  }, [conversation]);

  const target = useCallback(async () => {
    if (!conversation) return null;
    try {
      return targetFromThread(await ensureThread(conversation.thread_id));
    } catch (error) {
      toast.error("Couldn't open the conversation", {
        description: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  }, [conversation]);

  const makeTodo = useCallback(async () => {
    if (!conversation || !page || refuseWhileDaemonDown("add a to-do")) return;
    const first = page.row.person?.name?.split(/\s+/)[0] ?? page.row.title.split(",")[0];
    openMailDialog({
      kind: "todo-make",
      messageId: conversation.composer.reply_to_message_id,
      suggestion: first ? `Reply to ${first}` : "",
      subject: conversation.subject,
    });
  }, [conversation, page]);

  const replyLater = useCallback(async () => {
    if (refuseWhileDaemonDown("set reply later")) return;
    const found = await target();
    if (found) openMailDialog({ kind: "reply-later", target: found, waiting: false });
  }, [target]);

  const pin = useCallback(async () => {
    const row = page?.row ?? selected;
    if (!row?.person || refuseWhileDaemonDown("pin")) return;
    try {
      await setPinned(row.account_id, row.person.id, !row.pinned);
      toast.success(row.pinned ? `Unpinned ${row.title}.` : `Pinned ${row.title}.`);
    } catch (error) {
      toast.error("Couldn't change the pin", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
    await refreshMessages();
  }, [page?.row, selected]);

  const newTopic = useCallback(() => {
    const row = page?.row ?? selected;
    const to = row ? newTopicAddress(row) : null;
    if (!to) {
      toast.info("A new topic goes to one person; open a person, not a group.");
      return;
    }
    useComposeUi.getState().openCompose(prefilledMessageIntent(to), "overlay");
  }, [page?.row, selected]);

  const stepTopicBy = (delta: 1 | -1) => {
    if (!page || !selectedId) return;
    const next = stepTopic(page.topics, conversation?.thread_id ?? null, delta);
    if (next) select(selectedId, { topic: next.thread_id });
  };

  const latestTrimmed = () =>
    conversation?.messages.toReversed().find((message) => message.trimmed_label) ??
    conversation?.messages.at(-1);

  const index = Math.max(
    0,
    rows.findIndex((row) => row.id === selectedId),
  );
  const move = (delta: number) => {
    const next = rows[Math.min(rows.length - 1, Math.max(0, index + delta))];
    if (next) select(next.id);
  };
  useShortcutScope("messages", true);
  useScopeController("messages", {
    down: () => move(1),
    up: () => move(-1),
    open: () => selectedId && select(selectedId, { open: true }),
    personPage: () => {
      setPageOpen(true);
      document
        .querySelector<HTMLElement>(
          '[data-testid="person-page"] [data-testid="topic"][aria-current="true"]',
        )
        ?.focus();
    },
    reply: () => openReply(replyAll),
    replyAll: () => openReply(true),
    gotIt: () => conversation && void gotIt.start(conversation.thread_id),
    done: () => {
      retireCard();
      done();
    },
    makeTodo: () => void makeTodo(),
    replyLater: () => void replyLater(),
    pin: () => void pin(),
    newTopic,
    prevTopic: () => stepTopicBy(-1),
    nextTopic: () => stepTopicBy(1),
    asSent: () => {
      const message = latestTrimmed();
      if (message) setAsSent((open) => (open === message.message_id ? null : message.message_id));
    },
    sendNext: () => {
      const commands = useComposeUi.getState().commands;
      if (commands && replyKeys.current.has(commands.intentKey)) commands.send();
    },
    escape: () => {
      if (cardShown) retireCard();
      else setPageOpen(false);
    },
  });

  const empty = data.your_turn.length === 0;
  return (
    <ResizablePanelGroup className="min-h-0 flex-1" {...pane.groupProps}>
      <ResizablePanel
        id="messages-people"
        hidden={!split && pageOpen}
        {...pane.sidePanelProps}
        className="flex min-h-0 flex-col"
        elementRef={listRef}
      >
        <MessagesHeader guide={guide} turn={search.turn} />
        <div className="min-h-0 flex-1 overflow-y-auto pb-6">
          {cardShown && guide ? <ModeCard guide={guide} onClose={retireCard} /> : null}
          {empty && data.empty_state ? (
            <div data-testid="messages-empty" className="mx-5 mt-4">
              <p className="text-[14px] text-foreground">{data.empty_state}</p>
              {(data.lapsed ?? []).map((lapsed) => (
                <p
                  key={lapsed.person.id}
                  data-testid="lapsed-line"
                  className="mt-1 text-[13px] text-muted-foreground"
                >
                  {lapsed.line}
                </p>
              ))}
            </div>
          ) : null}
          <MessagesList
            data={data}
            selectedId={selectedId}
            quietOpen={quietOpen}
            onToggleQuiet={() => setQuietOpen((open) => !open)}
            onSelect={(id) => select(id, { open: true })}
            memberships={memberships}
          />
          {guide ? <KeyLine guide={guide} /> : null}
        </div>
      </ResizablePanel>
      {split ? <ResizableHandle aria-label="Resize people list" {...pane.handleProps} /> : null}
      {split || pageOpen ? (
        <ResizablePanel
          id="messages-conversation"
          {...pane.otherPanelProps}
          className="flex min-h-0 min-w-0"
        >
          {page ? (
            <PersonPane
              page={page}
              topic={topic}
              asSent={asSent}
              replyAll={replyAll}
              pending={gotIt.pending}
              ackLoading={gotIt.loading}
              onBack={() => setPageOpen(false)}
              onTopic={(thread) => selectedId && select(selectedId, { topic: thread })}
              onToggleAsSent={(id) => setAsSent((open) => (open === id ? null : id))}
              onReplyAllChange={setReplyAllOverride}
              onReply={() => openReply(replyAll)}
              onGotIt={() => conversation && void gotIt.start(conversation.thread_id)}
              onUndoGotIt={gotIt.undo}
              onDone={done}
              onTodo={() => void makeTodo()}
            />
          ) : person.isError ? (
            <Centered
              icon={<RefreshCw className="size-6" />}
              title="Couldn't open this person"
              body={person.error.message}
            />
          ) : selectedId ? (
            <div className="flex-1" aria-busy="true" />
          ) : (
            <Centered
              icon={<ChevronRight className="size-6" />}
              title="Nobody here yet"
              body={guide?.never_had_any}
            />
          )}
        </ResizablePanel>
      ) : null}
    </ResizablePanelGroup>
  );
}

/** Keys with their verbs at the point of use, from Messages' own guide. */
function KeyLine({ guide }: { guide: ModeGuide }) {
  const shown = new Set(["r", ".", "e", "t", "b", "c"]);
  return (
    <p className="mx-5 mt-6 hidden flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground md:flex">
      {guide.keys
        .filter((key) => shown.has(key.key))
        .map((key) => (
          <span key={key.key} className="inline-flex items-center gap-1">
            <KeyChip>{key.key}</KeyChip> {key.verb}
          </span>
        ))}
      <span className="inline-flex items-center gap-1">
        <KeyChip>?</KeyChip> what is this
      </span>
    </p>
  );
}
