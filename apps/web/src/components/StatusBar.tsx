import { RefreshCw } from "lucide-react";

import { ConnectionPill } from "@/components/ConnectionPill";
import { KeyChip } from "@/components/KeyChip";
import { syncNow } from "@/features/mailbox/actions";
import { useActionContext, useActionPrimaryHints } from "@/lib/actions";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useConnectionStore } from "@/state/connectionStore";
import { useKeyScope } from "@/state/keyScopeStore";
import { useModals } from "@/state/modalStore";

export function StatusBar() {
  const sync = useConnectionStore((s) => s.syncProgress);
  const reindex = useConnectionStore((s) => s.semanticReindexProgress);
  const pendingPrefix = useKeyScope((s) => s.pendingPrefix);
  const setHelpOpen = useModals((s) => s.setHelpOpen);
  const ctx = useActionContext();
  const hints = useActionPrimaryHints(ctx, 5);

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
          <span aria-hidden className="h-1 w-16 overflow-hidden rounded-full bg-muted">
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
          className="inline-flex items-center gap-1 rounded px-1 hover:bg-muted hover:text-foreground"
          aria-label="Sync now"
        >
          <RefreshCw className="size-3" />
          sync
        </button>
      )}
      {reindex ? (
        <span>semantic {Math.round((reindex.current / Math.max(1, reindex.total)) * 100)}%</span>
      ) : null}
      <span className="ml-auto flex min-w-0 items-center gap-3 overflow-hidden">
        {pendingPrefix ? (
          <span aria-live="polite" className="inline-flex items-center gap-1 text-foreground">
            <KeyChip className="border-primary/60 text-primary">{pendingPrefix}</KeyChip>…
          </span>
        ) : null}
        {hints.map((hint) => (
          <span key={hint.id} className={cn("hidden shrink-0 items-center gap-1 lg:inline-flex")}>
            <KeyChip>{hint.keys[0]}</KeyChip>
            <span>{hint.shortLabel.toLowerCase()}</span>
          </span>
        ))}
        <button
          type="button"
          onClick={() => setHelpOpen(true)}
          className="inline-flex shrink-0 items-center gap-1 rounded px-1 hover:bg-muted hover:text-foreground"
        >
          <KeyChip>?</KeyChip>
          <span>all keys</span>
        </button>
      </span>
    </>
  );
}
