import { useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import {
  prefilledMessageIntent,
  replyIntent,
  useComposeUi,
} from "@/features/compose/composeUiStore";
import { onSendEvent } from "@/features/compose/session/sendEvents";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { describeTarget } from "@/features/mail-actions/mailVerbs";
import { conversationNotNow } from "@/features/mail-actions/notNow";
import { ensureThread, targetFromThread } from "@/features/mail-actions/target";
import { PageEmpty, PageError } from "@/components/PageParts";
import { MessagesSkeleton } from "@/features/modes/ModeSkeletons";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { withDoneHereHint } from "@/features/hints/doneHereHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { FinishingMoment } from "@/features/low-tide/FinishingMoment";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { NextPlace } from "@/features/modes/NextPlace";
import { useThreadModesMap } from "@/features/modes/membership";
import { NEW_SENDER_NOTE } from "@/features/modes/NewSenderChips";
import type { LampVia } from "@/components/Row";
import type { LeaveVia } from "@/lib/leaving";
import { useLitReason } from "@/state/litReasonStore";
import { stripKeys } from "@/lib/actions/definition";
import { usePageKeys } from "@/state/pageKeysStore";
import { doneToast, markModeDone, useModeDone } from "@/features/modes/modeDone";
import { useAdvanceOnRemoval } from "@/hooks/useAdvanceOnRemoval";
import { moveCommands } from "@/features/arrivals/moveSubjects";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane } from "@/hooks/useSplitPane";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useScopeController } from "@/lib/keys/controllers";
import { focusActivePane } from "@/lib/keys/focusPane";

import {
  refreshMessages,
  LOADING_MESSAGES,
  setPinned,
  useMessagesQuery,
  rememberTopicPage,
  usePersonQuery,
  type MessagesData,
  type MessagesRow,
  type PersonPage,
} from "./api";
import { focusFilterBar, MessagesFilterBar } from "./MessagesFilterBar";
import {
  cycleMessagesFilter,
  filterOf,
  searchOf,
  type FilterSearch,
  type MessagesFilter,
} from "./filters";
import { MessagesList } from "./MessagesList";
import {
  afterDone,
  type Arrival,
  cursorRows,
  doneLine,
  newTopicAddress,
  openThreads,
  rowForThread,
  stepTopic,
  topicsLeft,
} from "./messagesView";
import { PersonPane } from "./PersonPane";
import { useGotIt } from "./useGotIt";
import { FramePanesContext, PageFrame } from "@/components/PageFrame";
import { listMotions } from "@/features/modes/listMotions";

const MESSAGES_SPLIT_QUERY = "(min-width: 768px)";
const PEOPLE_PANE_SIZE = {
  defaultSize: "22rem",
  minSize: "16rem",
  maxSize: "40rem",
};

const ARRIVAL_MS = 2_000;

export interface MessagesSearch extends FilterSearch {
  person?: string;
  topic?: string;
}

export function MessagesRoute({
  search,
  threadId,
}: {
  search: MessagesSearch;
  /** From /messages/$threadId: open the row holding this conversation. */
  threadId?: string;
}) {
  const navigate = useNavigate();
  const messages = useMessagesQuery(filterOf(search));
  useShortcutScope("messages-filters");
  useScopeController("messages-filters", {
    cycleFilter: () => {
      const person =
        search.person ??
        (threadId && messages.data ? rowForThread(messages.data, threadId)?.id : undefined);
      const nextSearch = (previous: MessagesSearch): MessagesSearch => ({
        person: previous.person ?? person,
        topic: previous.topic ?? threadId,
        ...searchOf(cycleMessagesFilter(filterOf(previous))),
      });
      if (threadId && !person) {
        void navigate({
          to: "/messages/$threadId",
          params: { threadId },
          search: nextSearch,
          replace: true,
        });
        return;
      }
      void navigate({
        to: "/messages",
        search: nextSearch,
        replace: true,
      });
    },
  });
  const guide = useModeGuide("messages");
  const loading = messages.isLoading || messages.data?.generated_at === "";
  const phase = useDelayedPending(loading, undefined, 0);
  return (
    <section
      aria-label="Messages"
      data-mode-pane=""
      tabIndex={-1}
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background outline-none"
    >
      {messages.isError ? (
        <>
          <MessagesHeader search={search} guide={guide.data} />
          <PageError
            framed
            title="Couldn't load Messages"
            error={messages.error}
            onRetry={() => void messages.refetch()}
          />
        </>
      ) : (
        <MessagesBody
          data={messages.data ?? LOADING_MESSAGES}
          stale={loading || messages.isPlaceholderData}
          loading={loading}
          skeleton={phase === "skeleton"}
          guide={guide.data}
          search={search}
          threadId={threadId}
        />
      )}
    </section>
  );
}

function MessagesHeader({
  search,
  guide,
  header,
}: {
  search: MessagesSearch;
  guide?: ModeGuide;
  header?: string;
}) {
  const waiting = search.turn === "theirs";
  return (
    <PageFrame
      title={waiting ? "Waiting on" : "Messages"}
      purpose={waiting ? "People you're waiting to hear from." : header || guide?.header}
    />
  );
}

function firstThread(row: MessagesRow | undefined): string {
  return row?.topics[0]?.thread_id ?? "";
}

function MessagesBody({
  data,
  stale,
  loading,
  skeleton,
  guide,
  search,
  threadId,
}: {
  data: MessagesData;
  stale: boolean;
  loading: boolean;
  skeleton: boolean;
  guide?: ModeGuide;
  search: MessagesSearch;
  threadId?: string;
}) {
  const navigate = useNavigate();
  const [quietOpen, setQuietOpen] = useState(false);
  const [pageOpen, setPageOpen] = useState(Boolean(search.person || threadId));
  const [filterFocusRequest, setFilterFocusRequest] = useState(0);
  useEffect(() => {
    if (filterFocusRequest > 0) focusFilterBar();
  }, [filterFocusRequest]);
  const [asSent, setAsSent] = useState<string | null>(null);
  const rows = useMemo(() => cursorRows(data, quietOpen), [data, quietOpen]);
  const firstTopics = useMemo(
    () =>
      [...data.your_turn, ...data.recent]
        .flatMap((row) => (row.topics[0] ? [row.topics[0].thread_id] : []))
        .slice(0, 100),
    [data],
  );
  const memberships = useThreadModesMap(firstTopics).data;

  const linked = threadId ? rowForThread(data, threadId) : undefined;
  const allRows = useMemo(
    () => [...data.your_turn, ...data.pinned, ...data.recent, ...data.quiet],
    [data],
  );
  const allRowIds = useMemo(() => new Set(allRows.map((row) => row.id)), [allRows]);
  const selectedId = search.person ?? linked?.id ?? null;
  const selected: MessagesRow | undefined = allRows.find((row) => row.id === selectedId);
  const topic = search.topic ?? (linked ? threadId : undefined) ?? null;
  const setFilter = useCallback(
    (next: MessagesFilter) => {
      void navigate({
        to: "/messages",
        search: (prev) => ({
          person: prev.person ?? selectedId ?? undefined,
          topic: prev.topic ?? topic ?? undefined,
          ...searchOf(next),
        }),
        replace: true,
      });
    },
    [navigate, selectedId, topic],
  );
  const person = usePersonQuery(selectedId, topic);
  const page: PersonPage | undefined = selectedId ? person.data : undefined;
  const conversation = page?.conversation ?? null;
  const hiddenDone = useModeDone((s) => s.hidden.messages);
  const [settled, setSettled] = useState<ReadonlySet<string>>(() => new Set());
  const excludedDone = useMemo(
    () => (settled.size === 0 ? hiddenDone : new Set([...hiddenDone, ...settled])),
    [hiddenDone, settled],
  );
  const stillOpen = useMemo(() => openThreads(selected, excludedDone), [selected, excludedDone]);
  const left = useMemo(
    () => (page ? topicsLeft(page.topics, stillOpen).length : 0),
    [stillOpen, page],
  );
  const [arrival, setArrival] = useState<Arrival | null>(null);
  useEffect(() => {
    if (!arrival) return;
    const timer = setTimeout(() => setArrival(null), ARRIVAL_MS);
    return () => clearTimeout(timer);
  }, [arrival]);
  const [replyAllOverride, setReplyAllOverride] = useState<boolean | null>(null);
  useEffect(() => {
    setPageOpen(Boolean(search.person || threadId));
  }, [search.person, threadId]);
  const replyAll = replyAllOverride ?? conversation?.composer.reply_all ?? false;
  useEffect(() => {
    setReplyAllOverride(null);
    setAsSent(null);
  }, [conversation?.thread_id]);

  const via = useRef<LampVia>("key");
  const select = useCallback(
    (id: string, options: { topic?: string; open?: boolean } = {}) => {
      void navigate({
        to: "/messages",
        search: {
          ...searchOf(filterOf(search)),
          person: id,
          topic: options.topic,
        },
        replace: true,
      });
      if (options.open) setPageOpen(true);
    },
    [navigate, search],
  );
  useEffect(() => {
    if (!stale && threadId && !linked) {
      void navigate({
        to: "/m/$mailbox/$threadId",
        params: { mailbox: "archive", threadId },
        replace: true,
      });
    }
  }, [linked, navigate, stale, threadId]);

  const shownTopic =
    person.isPlaceholderData || page?.row.id !== selectedId ? undefined : conversation?.thread_id;
  useEffect(() => {
    if (threadId || !search.person || search.topic || !shownTopic || !page) return;
    const current = stillOpen.has(shownTopic) ? shownTopic : stillOpen.values().next().value;
    if (!current) return;
    if (current === shownTopic) rememberTopicPage(search.person, current, page);
    select(search.person, { topic: current });
  }, [page, search.person, search.topic, select, shownTopic, stillOpen, threadId]);

  useEffect(() => {
    const id = search.person;
    if (id && data.quiet.some((row) => row.id === id)) setQuietOpen(true);
  }, [data.quiet, search.person]);

  const advanceTo = useCallback(
    (id: string, topicId?: string) => {
      select(id, { topic: topicId });
      setArrival({ key: Date.now(), person: id, thread: topicId ?? null });
    },
    [select],
  );
  const clearSelection = useCallback(() => {
    setPageOpen(false);
    void navigate({
      to: "/messages",
      search: searchOf(filterOf(search)),
      replace: true,
    });
  }, [navigate, search]);

  const previousTopic = useRef<{
    person: string | null;
    topic: string | null;
    open: boolean;
  }>({
    person: null,
    topic: null,
    open: false,
  });
  useEffect(() => {
    if (loading) return;
    if (settled.size === 0) return;
    const stillPresent = new Set(
      allRows.flatMap((row) =>
        row.topics.filter((t) => t.state !== "done").map((t) => t.thread_id),
      ),
    );
    const obsolete = [...settled].filter((id) => !stillPresent.has(id));
    if (obsolete.length > 0) {
      setSettled((prev) => {
        const next = new Set([...prev].filter((id) => !obsolete.includes(id)));
        return next.size === prev.size ? prev : next;
      });
    }
  }, [allRows, loading, settled]);
  useEffect(() => {
    if (loading) return;
    const previous = previousTopic.current;
    const open = topic !== null && stillOpen.has(topic);
    previousTopic.current = { person: selectedId, topic, open };
    if (
      !selectedId ||
      !topic ||
      !selected ||
      excludedDone.has(topic) ||
      previous.person !== selectedId ||
      previous.topic !== topic ||
      !previous.open ||
      open
    )
      return;
    const next = stillOpen.values().next().value;
    if (next) select(selectedId, { topic: next });
    else clearSelection();
  }, [clearSelection, excludedDone, loading, select, selected, selectedId, stillOpen, topic]);

  const rowIds = useMemo(() => rows.map((row) => row.id), [rows]);
  const listRef = useRef<HTMLDivElement>(null);
  // A callback ref into state, not a ref object: React attaches a parent's ref after its
  // children's layout effects, so the bands would mount without their scroller, and with
  // nothing else re-rendering (the daemon stopped) they would stay empty.
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const split = useMediaQuery(MESSAGES_SPLIT_QUERY);
  const pane = useSplitPane("messages", PEOPLE_PANE_SIZE, { active: split });
  const frames = useMemo(() => ({ split: split && pageOpen, inset: 0 }), [pageOpen, split]);
  const selectedBand = selected?.band;
  useEffect(() => {
    if (!selectedId) return;
    listRef.current
      ?.querySelector(`[data-row-id="${CSS.escape(selectedId)}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selectedId, selectedBand, quietOpen]);

  const paneVisible = useMediaQuery("(min-width: 768px)") || pageOpen;
  const topicsHint = useHint("messages", "messages.topics", {
    ready: paneVisible && (page?.topics.length ?? 0) > 1,
  });
  const [gotItNeeded, setGotItNeeded] = useState(false);
  const gotItHint = useHint("messages", "messages.got_it", {
    ready: paneVisible && gotItNeeded && conversation !== null,
  });
  const { dismiss: dismissGotItHint } = gotItHint;
  const closeHint = useActiveHintDismiss();

  const nextYourTurn = useCallback(() => {
    const next = data.your_turn.find((row) => row.id !== selectedId);
    if (next) advanceTo(next.id);
  }, [advanceTo, data.your_turn, selectedId]);

  const gotIt = useGotIt(`${selectedId ?? ""}|${conversation?.thread_id ?? ""}`, nextYourTurn);
  const startGotIt = () => {
    if (!conversation) return;
    dismissGotItHint();
    void gotIt.start(conversation.thread_id);
  };

  const replyKeys = useRef(new Set<string>());
  useEffect(
    () =>
      onSendEvent((event) => {
        if (!replyKeys.current.has(event.intentKey)) return;
        if (event.kind === "queued") nextYourTurn();
        if (event.kind === "sent") void refreshMessages();
      }),
    [nextYourTurn],
  );

  const openReply = useCallback(
    (all: boolean) => {
      if (!conversation) return;
      const intent = {
        ...replyIntent(conversation.composer.reply_to_message_id, all ? "all" : "single"),
        contextThreadId: conversation.thread_id,
        contextTitle: conversation.subject,
        quiet: true,
      };
      replyKeys.current.add(intent.key);
      setPageOpen(true);
      useComposeUi.getState().openCompose(intent, "inline");
    },
    [conversation],
  );

  const [resume, setResume] = useState<{
    person: string;
    thread: string;
    replyAll: boolean;
  } | null>(null);
  const openRow = useCallback(
    (id: string) => {
      const draft = allRows.find((row) => row.id === id)?.draft;
      setResume(draft ? { person: id, thread: draft.thread_id, replyAll: draft.reply_all } : null);
      select(id, { open: true, topic: draft?.thread_id });
    },
    [allRows, select],
  );
  const pageReady = Boolean(page && page.row.id === selectedId && !person.isPlaceholderData);
  const resumeReached = useRef(false);
  useEffect(() => {
    if (!resume) {
      resumeReached.current = false;
      return;
    }
    if (selectedId !== resume.person) {
      if (resumeReached.current) setResume(null);
      return;
    }
    resumeReached.current = true;
    if (!pageReady || conversation?.thread_id !== resume.thread) return;
    setResume(null);
    if (useComposeUi.getState().intent?.contextThreadId === resume.thread) return;
    openReply(resume.replyAll);
  }, [conversation?.thread_id, openReply, pageReady, resume, selectedId]);

  const here = useRef({
    mounted: true,
    person: search.person,
    topic: search.topic,
  });
  here.current.person = search.person;
  here.current.topic = search.topic;
  useEffect(() => {
    const current = here.current;
    current.mounted = true;
    return () => {
      current.mounted = false;
    };
  }, []);

  const done = useCallback(
    (how: LeaveVia = "key") => {
      if (!conversation || !page || !selectedId || person.isPlaceholderData) return;
      const thread = conversation.thread_id;
      if (page.row.id !== selectedId || (topic && topic !== thread) || !stillOpen.has(thread))
        return;
      const next = afterDone({
        topics: page.topics,
        thread,
        open: stillOpen,
        rows,
        person: selectedId,
        hidden: excludedDone,
      });
      const what =
        next.kind === "topic" ? { subject: conversation.subject } : { person: page.row.title };
      const from = selectedId;
      setSettled((prev) => (prev.has(thread) ? prev : new Set([...prev, thread])));
      const nextTopic =
        next.kind === "topic"
          ? next.topic.thread_id
          : next.kind === "person"
            ? openThreads(next.row, new Set([...excludedDone, thread]))
                .values()
                .next().value
            : undefined;
      const landed =
        next.kind === "topic"
          ? { person: from, topic: next.topic.thread_id }
          : next.kind === "person"
            ? { person: next.row.id, topic: nextTopic }
            : { person: undefined, topic: undefined };
      if (next.kind === "topic") advanceTo(from, next.topic.thread_id);
      else if (next.kind === "person") advanceTo(next.row.id, nextTopic);
      else clearSelection();
      const account = useUiPrefs.getState().accountScope;
      const label = "subject" in what ? what.subject : what.person;
      void markModeDone("messages", [thread], {
        via: how,
        message: withDoneHereHint((outcomes) => doneLine(what, doneToast(outcomes), next)),
        onUndone: () => {
          setSettled((prev) => {
            if (!prev.has(thread)) return prev;
            const nextSet = new Set(prev);
            nextSet.delete(thread);
            return nextSet;
          });
          const now = here.current;
          const stayed =
            now.mounted &&
            useUiPrefs.getState().accountScope === account &&
            (now.person === from || (now.person === landed.person && now.topic === landed.topic));
          if (stayed) {
            advanceTo(from, thread);
            return;
          }
          toast.info(`Restored: ${label}`, {
            action: {
              label: "Open",
              onClick: () => {
                if (useUiPrefs.getState().accountScope !== account) {
                  useUiPrefs.getState().setAccountScope(account);
                }
                void navigate({
                  to: "/messages",
                  search: { person: from, topic: thread },
                });
              },
            },
          });
        },
      }).then((succeeded) => {
        if (!succeeded) {
          setSettled((prev) => {
            if (!prev.has(thread)) return prev;
            const nextSet = new Set(prev);
            nextSet.delete(thread);
            return nextSet;
          });
        }
      });
    },
    [
      advanceTo,
      clearSelection,
      conversation,
      excludedDone,
      navigate,
      page,
      person.isPlaceholderData,
      rows,
      selectedId,
      stillOpen,
      topic,
    ],
  );

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
    if (!found?.threadId) return;
    openMailDialog({
      kind: "not-now",
      picker: conversationNotNow({
        threadId: found.threadId,
        messageId: found.primary?.id,
        subject: describeTarget(found),
        waiting: false,
      }),
    });
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
    topicsHint.dismiss();
    const next = stepTopic(page.topics, conversation?.thread_id ?? null, delta);
    if (next) select(selectedId, { topic: next.thread_id });
  };

  const latestTrimmed = () =>
    conversation?.messages.toReversed().find((message) => message.trimmed_label) ??
    conversation?.messages.at(-1);

  const index = rows.findIndex((row) => row.id === selectedId);
  const move = (delta: number) => {
    const nextIndex = index < 0 ? (delta > 0 ? 0 : rows.length - 1) : index + delta;
    const next = rows[Math.min(rows.length - 1, Math.max(0, nextIndex))];
    if (!next) return;
    via.current = "key";
    select(next.id);
  };
  const newSender = selectedId ? memberships?.get(firstThread(selected))?.new_sender : undefined;
  useLitReason(selected?.why, newSender ? NEW_SENDER_NOTE : null, selected?.account_id);
  usePageKeys(stripKeys(selectedId ? "messages" : "messages.empty"));
  useShortcutScope("messages", true);
  useScopeController("messages", {
    down: () => move(1),
    up: () => move(-1),
    ...listMotions(move),
    top: () => move(rows.length > 0 && index < 0 ? 1 : -Number.MAX_SAFE_INTEGER),
    filter: () => {
      setPageOpen(false);
      setFilterFocusRequest((request) => request + 1);
    },
    open: () => selectedId && openRow(selectedId),
    personPage: () => {
      if (!selectedId) return;
      setPageOpen(true);
      requestAnimationFrame(() => {
        const personPage = document.querySelector<HTMLElement>('[data-testid="person-page"]');
        const currentTopic = personPage?.querySelector<HTMLElement>(
          '[data-testid="topic"][aria-current="true"]',
        );
        const topicTrigger = personPage?.querySelector<HTMLElement>(
          '[data-testid="topics-trigger"]',
        );
        (currentTopic ?? topicTrigger)?.focus();
      });
    },
    reply: () => openReply(replyAll),
    replyAll: () => openReply(true),
    gotIt: startGotIt,
    done: () => done("key"),
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
      if (closeHint) {
        closeHint();
        return;
      }
      setPageOpen(false);
      if (document.activeElement?.closest('[data-testid="person-page"]')) focusActivePane();
    },
    ...moveCommands(() => {
      const theirs = conversation?.messages.findLast((message) => !message.from_me);
      return theirs && conversation
        ? {
            messageId: theirs.message_id,
            label: conversation.subject || "(no subject)",
            senderLabel: theirs.from.name || theirs.from.email,
            newSender: newSender
              ? { question: newSender, label: selected?.title ?? "" }
              : undefined,
            here: "messages" as const,
          }
        : null;
    }),
  });

  const latest = useRef({ openRow, openReply, done, replyAll });
  latest.current = { openRow, openReply, done, replyAll };
  const onSelectRow = useCallback((id: string) => {
    via.current = "pointer";
    latest.current.openRow(id);
  }, []);
  const onReplyRow = useCallback(() => latest.current.openReply(latest.current.replyAll), []);
  const onDoneRow = useCallback(() => latest.current.done("pointer"), []);

  const empty = !stale && data.empty_state != null;
  const filter = filterOf(search);
  const filterBar = (
    <MessagesFilterBar
      filter={filter}
      counts={stale ? null : { matched: data.matched ?? data.row_count, total: data.row_count }}
      onChange={setFilter}
    />
  );
  const narrowed = filter.unread || filter.pastPace;
  const yourTurnSplit = data.your_turn_split;
  const yourTurnPeople = (yourTurnSplit?.write_with ?? 0) + (yourTurnSplit?.others ?? 0);
  const hasWork = narrowed ? yourTurnPeople > 0 : data.your_turn.length > 0 || !empty;
  const finished = useLowTide("messages", loading || search.turn === "theirs", hasWork, {
    daily: true,
  });
  const emptyState = data.empty_state ? (
    <PageEmpty
      framed={allRows.length === 0}
      glyph={finished ? undefined : "messages"}
      moment={finished ? <FinishingMoment /> : undefined}
      title={data.empty_state}
      body={
        <>
          {(data.lapsed ?? []).map((lapsed) => (
            <span key={lapsed.person.id} data-testid="lapsed-line" className="block">
              {lapsed.line}
            </span>
          ))}
          <NextPlace current="messages" />
        </>
      }
    />
  ) : null;
  return (
    <FramePanesContext.Provider value={frames}>
      <SelectionAdvance
        key={JSON.stringify(filter)}
        ids={rowIds}
        ready={!stale}
        selectedId={search.person ?? null}
        present={allRowIds}
        onAdvance={advanceTo}
        onEmpty={clearSelection}
      />
      <ResizablePanelGroup className="min-h-0 flex-1" {...pane.groupProps}>
        <ResizablePanel
          id="messages-people"
          hidden={!split && pageOpen}
          {...pane.sidePanelProps}
          className="flex min-h-0 flex-col"
          elementRef={listRef}
        >
          <MessagesHeader search={search} guide={guide} header={data.header} />
          {filterBar}
          <div
            ref={setScroller}
            aria-busy={loading}
            className="min-h-0 flex-1 overflow-y-auto pb-6"
          >
            {empty && emptyState ? (
              <div data-testid="messages-empty" className="mx-5 mt-6">
                {emptyState}
              </div>
            ) : null}
            {loading ? (
              <MessagesSkeleton shown={skeleton} />
            ) : (
              <MessagesList
                data={data}
                selectedId={selectedId}
                via={via.current}
                quietOpen={quietOpen}
                onToggleQuiet={() => setQuietOpen((open) => !open)}
                scroller={scroller}
                onSelect={onSelectRow}
                onReply={onReplyRow}
                onDone={onDoneRow}
                memberships={memberships}
                hidden={excludedDone}
                arrival={arrival}
              />
            )}
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
              <div className="reader-arrive flex min-h-0 min-w-0 flex-1">
                <PersonPane
                  page={page}
                  topic={topic}
                  asSent={asSent}
                  replyAll={replyAll}
                  left={left}
                  hidden={excludedDone}
                  arrival={arrival?.person === selectedId ? arrival : null}
                  loading={person.isPlaceholderData}
                  pending={gotIt.pending}
                  ackLoading={gotIt.loading}
                  onBack={clearSelection}
                  onTopic={(thread) => {
                    topicsHint.dismiss();
                    if (selectedId) select(selectedId, { topic: thread });
                  }}
                  onToggleAsSent={(id) => setAsSent((open) => (open === id ? null : id))}
                  onReplyAllChange={setReplyAllOverride}
                  onReply={() => openReply(replyAll)}
                  onGotIt={startGotIt}
                  onGotItFocus={() => setGotItNeeded(true)}
                  topicsHint={
                    topicsHint.hint ? (
                      <AnchoredHint hint={topicsHint.hint} onDismiss={topicsHint.dismiss} />
                    ) : undefined
                  }
                  gotItHint={
                    gotItHint.hint ? (
                      <AnchoredHint hint={gotItHint.hint} onDismiss={gotItHint.dismiss} />
                    ) : undefined
                  }
                  onUndoGotIt={gotIt.undo}
                  onDone={() => done("pointer")}
                  onTodo={() => void makeTodo()}
                />
              </div>
            ) : person.isError ? (
              <PageError
                framed
                title="Couldn't open this person"
                subject="this person"
                error={person.error}
                onRetry={() => void person.refetch()}
              />
            ) : selectedId ? (
              <div className="flex-1" aria-busy="true" />
            ) : allRows.length > 0 || loading ? (
              <PageEmpty
                framed
                title="Choose a conversation"
                body="Select someone from the list to open their conversations."
              />
            ) : null}
          </ResizablePanel>
        ) : null}
      </ResizablePanelGroup>
    </FramePanesContext.Provider>
  );
}

function SelectionAdvance({
  ids,
  ready,
  selectedId,
  present,
  onAdvance,
  onEmpty,
}: {
  ids: readonly string[];
  ready: boolean;
  selectedId: string | null;
  present: ReadonlySet<string>;
  onAdvance: (id: string) => void;
  onEmpty: () => void;
}) {
  useAdvanceOnRemoval({
    ids,
    ready,
    selectedId,
    isPresent: (id) => present.has(id),
    presenceSource: present,
    onAdvance: (id) => onAdvance(id),
    onEmpty,
  });
  return null;
}
