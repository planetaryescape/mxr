import { X } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { AttachmentActions } from "@/features/thread/AttachmentActions";
import type { AttachmentView } from "@/features/mailbox/types";
import { useModals } from "@/state/modalStore";

export function RightRail() {
  const rail = useModals((s) => s.rightRail);
  const close = useModals((s) => s.closeRightRail);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && rail) close();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [rail, close]);

  if (!rail) return null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 items-center justify-between border-b border-border px-3">
        <div className="text-xs font-medium capitalize">{rail.kind.replace(/-/g, " ")}</div>
        <Button variant="ghost" size="icon" onClick={close} aria-label="Close panel">
          <X className="size-3" />
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="p-3 text-xs text-muted-foreground">
          <RailContent kind={rail.kind} payload={rail.payload} />
        </div>
      </ScrollArea>
    </div>
  );
}

function RailContent({ kind, payload }: { kind: string; payload: unknown }) {
  if (kind === "thread-context" && isThreadContext(payload)) {
    return (
      <div className="space-y-3">
        <h3 className="text-sm font-medium text-foreground">{payload.title ?? "Thread context"}</h3>
        <ul className="space-y-2">
          {(payload.items ?? []).map((item) => (
            <li key={item} className="rounded-md border border-border bg-muted/40 px-3 py-2">
              {item}
            </li>
          ))}
        </ul>
      </div>
    );
  }
  if (kind === "attachments" && Array.isArray(payload)) {
    return (
      <div className="space-y-2">
        {payload.map((item, index) => {
          const attachment = item as AttachmentView;
          return <AttachmentActions key={attachment.id ?? index} attachment={attachment} />;
        })}
      </div>
    );
  }
  return <pre className="font-mono text-2xs">{JSON.stringify(payload ?? null, null, 2)}</pre>;
}

function isThreadContext(value: unknown): value is { title?: string; items?: string[] } {
  return typeof value === "object" && value !== null && "items" in value;
}
