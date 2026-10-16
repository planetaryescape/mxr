import { RefreshCw } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";

import { ConnectionPill } from "@/components/ConnectionPill";
import { KeyChip } from "@/components/KeyChip";
import { InstallOffer } from "@/features/install/InstallOffer";
import { useSyncHealthToasts } from "@/features/freshness/AccountSyncHealth";
import { useFreshnessLive } from "@/features/freshness/api";
import { FreshnessIndicator } from "@/features/freshness/FreshnessIndicator";
import { syncNow } from "@/features/mailbox/actions";
import {
  pageKeyHints,
  useActionContext,
  useActionPrimaryHints,
  type ShortcutHint,
} from "@/lib/actions";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useConnectionStore } from "@/state/connectionStore";
import { useKeyScope } from "@/state/keyScopeStore";
import { useLitReasonStore } from "@/state/litReasonStore";
import { useModals } from "@/state/modalStore";
import { usePageKeysStore } from "@/state/pageKeysStore";

export function StatusBar() {
  const sync = useConnectionStore((s) => s.syncProgress);
  const reindex = useConnectionStore((s) => s.semanticReindexProgress);
  const pendingPrefix = useKeyScope((s) => s.pendingPrefix);
  const setHelpOpen = useModals((s) => s.setHelpOpen);
  const ctx = useActionContext();
  const pageKeys = usePageKeysStore((s) => s.ids);
  const fallback = useActionPrimaryHints(ctx, 5);
  // A page that named its verbs (`usePageKeys`) owns the strip; the rest get the registry's live keys.
  const hints = pageKeys.length > 0 ? pageKeyHints(pageKeys, ctx) : fallback;
  const reason = useLitReasonStore((s) => s.reason);
  const note = useLitReasonStore((s) => s.note);
  // The status bar is mounted on every page, so it owns the one live
  // subscription; the phone's topbar indicator reads the same query.
  useFreshnessLive();
  useSyncHealthToasts();

  return (
    <>
      <ConnectionPill />
      <span aria-hidden className="text-faint">
        │
      </span>
      {sync ? (
        <span className="inline-flex items-center gap-1.5" role="status">
          <RefreshCw className="size-3 animate-spin text-primary" />
          <span>
            syncing {sync.total > 0 ? `${sync.current}/${plural(sync.total, "message")}` : "…"}
          </span>
          <span aria-hidden className="h-1 w-16 overflow-hidden bg-muted">
            <span
              className="block h-full origin-left bg-primary transition-transform"
              style={{
                transform: `scaleX(${sync.total > 0 ? Math.min(1, sync.current / sync.total) : 0.05})`,
              }}
            />
          </span>
        </span>
      ) : (
        <button
          type="button"
          onClick={() => void syncNow()}
          className="inline-flex items-center gap-1 rounded-sm px-1 hover:bg-muted hover:text-foreground"
          aria-label="Sync now"
        >
          <RefreshCw className="size-3" />
          sync
        </button>
      )}
      {reindex ? (
        <span>
          Search index {Math.round((reindex.current / Math.max(1, reindex.total)) * 100)}%
        </span>
      ) : null}
      <span aria-hidden className="text-faint">
        │
      </span>
      <FreshnessIndicator reason={reason} note={note} />
      <span className="ml-auto flex min-w-0 shrink items-center gap-3">
        {pendingPrefix ? (
          <span aria-live="polite" className="inline-flex items-center gap-1 text-foreground">
            <KeyChip className="border-primary/60 text-primary">{pendingPrefix}</KeyChip>…
          </span>
        ) : null}
        <KeyStrip hints={hints} />
        <InstallOffer />
        <button
          type="button"
          onClick={() => setHelpOpen(true)}
          className="inline-flex shrink-0 items-center gap-1 rounded-sm px-1 hover:bg-muted hover:text-foreground"
        >
          <KeyChip>?</KeyChip>
          <span>keys</span>
        </button>
      </span>
    </>
  );
}

/**
 * The page's verbs. Each is nowrap and the row wraps inside a one-line box,
 * so whatever doesn't fit drops whole from the end and no word is cut. The
 * one case wrapping can't cover is a box narrower than the first verb: then
 * the whole strip steps aside. A resize observer decides that, and the state
 * changes only when the width crosses it, so a resize re-renders nothing else.
 */
function KeyStrip({ hints }: { hints: readonly ShortcutHint[] }) {
  const box = useRef<HTMLUListElement>(null);
  const [fits, setFits] = useState(true);
  // `hints` is a new array each render; the effect follows what it names, not its identity.
  const signature = hints.map((hint) => hint.id).join("\u0000");
  useLayoutEffect(() => {
    const node = box.current;
    if (!node) return;
    const measure = () => {
      const first = node.firstElementChild;
      setFits(!first || node.clientWidth >= first.getBoundingClientRect().width - 0.5);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [signature]);
  return (
    <ul
      ref={box}
      aria-label="Keys on this page"
      data-testid="status-keys"
      className={cn(
        "hidden h-5 min-w-0 flex-wrap justify-end gap-x-3 overflow-hidden lg:flex",
        !fits && "invisible",
      )}
    >
      {hints.map((hint) => (
        <li key={hint.id} className="inline-flex h-5 shrink-0 items-center gap-1 whitespace-nowrap">
          <KeyChip>{hint.keys[0]}</KeyChip>
          <span>{hint.shortLabel.toLowerCase()}</span>
        </li>
      ))}
    </ul>
  );
}
