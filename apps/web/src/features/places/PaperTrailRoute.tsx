import { useParams } from "@tanstack/react-router";
import { ChevronRight, Pin, Receipt, RefreshCw } from "lucide-react";
import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { performMailAction } from "@/features/mail-actions/mailMutations";
import { targetFromRows } from "@/features/mail-actions/target";
import { SwipeLayer, useRowSwipe, type SwipeLayerHandle } from "@/features/swipe/RowSwipe";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { notePointerUse } from "@/lib/actions/keyHints";
import { formatLongDate, plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import type { PlaceBundle } from "./api";
import { PlaceHeader, PlaceLayout } from "./PlaceLayout";
import { bundleKey, bundleSender, paperTrailItems, whyHere, type PlaceItem } from "./placeCopy";
import { messagesLeft } from "./placePaging";
import {
  openMoveSender,
  openSweep,
  openUnsubscribe,
  placeMessageRow,
  togglePin,
} from "./placeVerbs";
import { SweptClear } from "./SweptClear";
import { usePlace } from "./usePlace";
import { When } from "@/components/When";

/** Messages listed per bundle when it opens, and per "more from" page. */
const MESSAGES_PER_BUNDLE = 20;

type PlaceState = ReturnType<typeof usePlace>;

/** Unpinned messages on screen: those of the open bundles. */
function shownUnpinned(bundles: readonly PlaceBundle[], open: (bundle: PlaceBundle) => boolean) {
  return bundles
    .filter(open)
    .reduce((sum, bundle) => sum + bundle.messages.filter((m) => !m.pinned).length, 0);
}

/**
 * Paper trail: receipts, notifications and automated mail, one row per
 * sender. A row opens to its messages; pin the few that matter and sweep
 * the rest. Counts here are facts, never badges: none of this is work.
 */
export function PaperTrailRoute() {
  const place = usePlace("paper_trail", MESSAGES_PER_BUNDLE);
  const { bundles, status } = place;
  // Open bundles are what is on screen, which the sweep preview counts.
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const threadIds = useCallback(
    () => [...new Set(bundles.flatMap((bundle) => bundle.messages.map((m) => m.thread_id)))],
    [bundles],
  );
  const total = place.totalMessages;
  const senders = place.totalBundles;
  const phase = useDelayedPending(status.isLoading);
  const lowTide = useLowTide("paper_trail", status.isLoading || status.isError, bundles.length > 0);
  return (
    <PlaceLayout basePath="/paper-trail" label="Paper trail" threadIds={threadIds}>
      <PlaceHeader
        title="Paper trail"
        meta={
          total > 0
            ? `${plural(total, "message")} from ${plural(senders, "sender")}. Pin what matters, sweep the rest.`
            : "Receipts, notifications and automated mail, bundled by sender."
        }
        actions={
          bundles.length > 0 ? (
            <Button
              variant="outline"
              size="xs"
              onClick={() => {
                openSweep(
                  "paper_trail",
                  place.accountId,
                  undefined,
                  shownUnpinned(bundles, (bundle) => expanded.has(bundleKey(bundle))),
                );
                notePointerUse("place.sweep-all");
              }}
            >
              Sweep all <KeyChip className="ml-1 h-4 px-1">A</KeyChip>
            </Button>
          ) : null
        }
      />
      {phase !== "ready" ? (
        <ListSkeleton quiet={phase === "quiet"} />
      ) : status.isError ? (
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't load Paper trail"
          body={status.error.message}
          action={
            <Button size="sm" onClick={() => void status.refetch()}>
              Try again
            </Button>
          }
        />
      ) : bundles.length === 0 && lowTide ? (
        <SweptClear place="Paper trail" />
      ) : bundles.length === 0 ? (
        <Centered
          icon={<Receipt className="size-6" />}
          title="Paper trail is clear"
          body="Receipts and notifications that reach your inbox gather here, one line per sender."
        />
      ) : (
        <Bundles place={place} expanded={expanded} setExpanded={setExpanded} />
      )}
    </PlaceLayout>
  );
}

function Bundles({
  place,
  expanded,
  setExpanded,
}: {
  place: PlaceState;
  expanded: ReadonlySet<string>;
  setExpanded: Dispatch<SetStateAction<ReadonlySet<string>>>;
}) {
  const { bundles } = place;
  const account = useUiPrefs((s) => s.accountScope);
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const [cursorKey, setCursorKey] = useState<string | null>(null);
  const items = useMemo(() => paperTrailItems(bundles, expanded), [bundles, expanded]);
  const index = Math.max(
    0,
    items.findIndex((item) => item.key === cursorKey),
  );
  const current: PlaceItem | undefined = items[index];
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  // Touch: swipe a sender right to preview sweeping it; swipe one message
  // to archive, trash (long throw) or snooze it, with the usual undo.
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const swipeLayer = useRef<SwipeLayerHandle>(null);
  useRowSwipe(
    listRef,
    {
      rowSelector: '[data-testid="place-bundle"], [data-testid="place-message"]',
      layer: swipeLayer,
      resolve: (element) => {
        const item = itemsRef.current[Number(element.dataset.index)];
        if (!item) return null;
        if (item.type === "bundle") {
          return {
            actions: { right: "sweep" },
            commit: () => openSweep("paper_trail", account, item.bundle),
          };
        }
        const ids = [item.message.message_id];
        return {
          actions: { right: "archive", rightLong: "trash", left: "snooze" },
          commit: (action) => {
            if (action === "archive" || action === "trash") void performMailAction(action, ids);
            else if (action === "snooze") {
              openMailDialog({
                kind: "snooze",
                target: targetFromRows([placeMessageRow(item.bundle, item.message)], "list"),
              });
            }
          },
        };
      },
    },
    items.length > 0,
  );

  const toggle = useCallback(
    (key: string) => {
      setExpanded((previous) => {
        const next = new Set(previous);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      });
    },
    [setExpanded],
  );

  const activate = useCallback(
    (item: PlaceItem) => {
      setCursorKey(item.key);
      if (item.type === "bundle") toggle(item.key);
      else nav?.open(item.message.thread_id);
    },
    [nav, toggle],
  );

  const move = (delta: number) => {
    const next = items[Math.min(items.length - 1, Math.max(0, index + delta))];
    if (next) setCursorKey(next.key);
  };

  // The reader keeps its own keys while it has focus.
  useShortcutScope("place", !params.threadId || activePane !== "reader");
  useScopeController("place", {
    down: () => move(1),
    up: () => move(-1),
    open: () => current && activate(current),
    pin: () => {
      if (!current) return;
      if (current.type === "message") {
        void togglePin(current.message);
        return;
      }
      // Pins are per message: open the bundle to choose one.
      if (!expanded.has(current.key)) toggle(current.key);
    },
    sweepBundle: () =>
      current &&
      openSweep(
        "paper_trail",
        account,
        current.bundle,
        shownUnpinned([current.bundle], (bundle) => expanded.has(bundleKey(bundle))),
      ),
    sweepAll: () =>
      openSweep(
        "paper_trail",
        account,
        undefined,
        shownUnpinned(bundles, (bundle) => expanded.has(bundleKey(bundle))),
      ),
    moveSender: () => current && openMoveSender(current.bundle),
    unsubscribe: () => {
      if (!current) return;
      const message = current.type === "message" ? current.message : current.bundle.messages[0];
      if (message) openUnsubscribe(current.bundle, message);
    },
  });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <SwipeLayer ref={swipeLayer} />
      <ul
        ref={listRef}
        aria-label="Senders in Paper trail"
        className="max-w-[64rem] py-1 [touch-action:pan-y_pinch-zoom]"
      >
        {items.map((item, position) => {
          if (item.type === "bundle") {
            return (
              <BundleRow
                key={item.key}
                index={position}
                bundle={item.bundle}
                focused={position === index}
                open={expanded.has(item.key)}
                onActivate={() => activate(item)}
              />
            );
          }
          const lastOfBundle = items[position + 1]?.type !== "message";
          return (
            <Fragment key={item.key}>
              <MessageRow
                index={position}
                item={item}
                focused={position === index}
                active={params.threadId === item.message.thread_id}
                onActivate={() => activate(item)}
              />
              {lastOfBundle ? (
                <BundleActions
                  bundle={item.bundle}
                  account={account}
                  onMore={() => place.moreFrom([item.bundle])}
                  loadingMore={place.loadingFrom.has(bundleKey(item.bundle))}
                />
              ) : null}
            </Fragment>
          );
        })}
      </ul>
      {place.hasMoreSenders ? (
        <div className="max-w-[64rem] px-5 py-3">
          <Button
            variant="outline"
            size="sm"
            disabled={place.loadingMoreSenders}
            onClick={place.loadMoreSenders}
          >
            {place.loadingMoreSenders
              ? "Loading…"
              : `Show more senders (${bundles.length.toLocaleString()} of ${place.totalBundles.toLocaleString()} here)`}
          </Button>
        </div>
      ) : null}
      <p className="hidden max-w-[64rem] flex-wrap md:flex items-center gap-x-3 gap-y-1 px-5 py-3 text-[12px] text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <KeyChip>Enter</KeyChip> open
        </span>
        <span className="inline-flex items-center gap-1">
          <KeyChip>p</KeyChip> pin
        </span>
        <span className="inline-flex items-center gap-1">
          <KeyChip>S</KeyChip> sweep sender
        </span>
        <span className="inline-flex items-center gap-1">
          <KeyChip>K</KeyChip> move sender
        </span>
      </p>
    </div>
  );
}

const BundleRow = memo(function BundleRow({
  index,
  bundle,
  focused,
  open,
  onActivate,
}: {
  index: number;
  bundle: PlaceBundle;
  focused: boolean;
  open: boolean;
  onActivate: () => void;
}) {
  const sender = bundleSender(bundle);
  return (
    <li
      data-index={index}
      data-testid="place-bundle"
      data-sender={bundle.sender_email}
      aria-current={focused ? "true" : undefined}
      className="relative"
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-1.5 left-0 w-[2px] rounded-full bg-primary" />
      ) : null}
      <button
        type="button"
        aria-expanded={open}
        onClick={onActivate}
        className={cn(
          "mx-2 grid w-[calc(100%-1rem)] cursor-default grid-cols-[1rem_minmax(0,1fr)_auto] items-baseline gap-x-2 rounded-md px-3 py-2 text-left",
          focused ? "bg-accent" : "hover:bg-accent/40",
        )}
      >
        <ChevronRight
          aria-hidden
          className={cn("size-3.5 translate-y-0.5 text-muted-foreground", open && "rotate-90")}
        />
        <span className="min-w-0 truncate text-[13px]">
          <span className="font-medium text-foreground">{sender}</span>
          <span className="ml-2 font-mono text-2xs tabular-nums text-muted-foreground">
            {bundle.message_count}
          </span>
          <span className="ml-3 text-muted-foreground">{bundle.newest_subject}</span>
        </span>
        <time
          dateTime={bundle.newest_at}
          title={formatLongDate(bundle.newest_at)}
          className="whitespace-nowrap font-mono text-2xs tabular-nums text-muted-foreground"
        >
          {bundle.pinned_count > 0 ? (
            <Pin aria-label={`${bundle.pinned_count} pinned`} className="mr-1.5 inline size-3" />
          ) : null}
          <When value={bundle.newest_at} />
        </time>
        <span className="col-start-2 col-end-4 truncate text-[12px] text-muted-foreground/90">
          {whyHere(bundle.kind)}
        </span>
      </button>
    </li>
  );
});

const MessageRow = memo(function MessageRow({
  index,
  item,
  focused,
  active,
  onActivate,
}: {
  index: number;
  item: Extract<PlaceItem, { type: "message" }>;
  focused: boolean;
  active: boolean;
  onActivate: () => void;
}) {
  const { message } = item;
  return (
    <li
      data-index={index}
      data-testid="place-message"
      data-pinned={message.pinned ? "true" : undefined}
      aria-current={focused ? "true" : undefined}
      className={cn(
        "relative mx-2 flex items-center gap-1 rounded-md pl-9 pr-1",
        focused ? "bg-accent" : active ? "bg-accent/60" : "hover:bg-accent/40",
      )}
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-1 -left-2 w-[2px] rounded-full bg-primary" />
      ) : null}
      <button
        type="button"
        onClick={onActivate}
        className="grid min-w-0 flex-1 cursor-default grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-2 py-1.5 text-left text-[13px]"
      >
        <span className="min-w-0 truncate text-foreground/90">
          {message.subject || "(no subject)"}
        </span>
        <time
          dateTime={message.date}
          className="whitespace-nowrap font-mono text-2xs tabular-nums text-muted-foreground"
        >
          <When value={message.date} />
        </time>
      </button>
      <button
        type="button"
        aria-pressed={message.pinned}
        aria-label={message.pinned ? "Unpin" : "Pin"}
        title={message.pinned ? "Unpin (p)" : "Pin: a sweep leaves it here (p)"}
        onClick={() => void togglePin(message)}
        className={cn(
          "grid size-7 shrink-0 place-items-center rounded-md hover:bg-accent",
          message.pinned ? "text-primary" : "text-muted-foreground/60 hover:text-foreground",
        )}
      >
        <Pin className={cn("size-3.5", message.pinned && "fill-current")} />
      </button>
    </li>
  );
});

/** Under an open bundle: the same verbs as the keys, for pointer and touch. */
function BundleActions({
  bundle,
  account,
  onMore,
  loadingMore,
}: {
  bundle: PlaceBundle;
  account: string | null;
  onMore: () => void;
  loadingMore: boolean;
}) {
  const unpinned = bundle.message_count - bundle.pinned_count;
  const shown = bundle.messages.filter((message) => !message.pinned).length;
  const left = messagesLeft(bundle);
  const first = bundle.messages[0];
  return (
    <li className="mx-2 flex flex-wrap items-center gap-x-4 gap-y-1 pb-3 pl-9 pt-1 text-[12px] text-muted-foreground">
      {left > 0 ? (
        <button
          type="button"
          onClick={onMore}
          disabled={loadingMore}
          className="hover:text-foreground disabled:opacity-60"
        >
          {loadingMore ? "Loading…" : `${plural(left, "more message")} from this sender`}
        </button>
      ) : null}
      {unpinned > 0 ? (
        <button
          type="button"
          onClick={() => {
            openSweep("paper_trail", account, bundle, shown);
            notePointerUse("place.sweep-bundle");
          }}
          className="hover:text-foreground"
        >
          Sweep {plural(unpinned, "message")}
        </button>
      ) : null}
      <button
        type="button"
        onClick={() => openMoveSender(bundle)}
        className="hover:text-foreground"
      >
        Move sender…
      </button>
      {first ? (
        <button
          type="button"
          onClick={() => openUnsubscribe(bundle, first)}
          className="hover:text-foreground"
        >
          Unsubscribe
        </button>
      ) : null}
    </li>
  );
}
