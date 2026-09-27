import { X } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { AskArchivePanel } from "@/features/ask/AskArchivePanel";
import { WhoisPanel, isWhoisPayload } from "@/features/ask/WhoisPanel";
import { ExpertFinderPanel } from "@/features/mailbox/ExpertFinderPanel";
import { AttachmentActions } from "@/features/thread/AttachmentActions";
import { DraftAssistPanel } from "@/features/thread/DraftAssistPanel";
import { useModals } from "@/state/modalStore";
import { BriefingPanel } from "./right-rail/BriefingPanel";
import { CommitmentsPanel } from "./right-rail/CommitmentsPanel";
import { SenderProfilePanel } from "./right-rail/SenderProfilePanel";
import { isAttachmentView, isDraftAssistPayload, isThreadContext } from "./right-rail/railPayloads";

export function RightRail() {
  const rail = useModals((s) => s.rightRail);
  const close = useModals((s) => s.closeRightRail);

  // Escape closes the innermost thing first: the rail, before the key
  // dispatcher sees it and closes the conversation underneath.
  useEffect(() => {
    if (!rail) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      const target = event.target;
      if (target instanceof Element && target.closest('[role="dialog"], input, textarea')) return;
      event.preventDefault();
      close();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [rail, close]);

  if (!rail) return null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-border pl-4 pr-2">
        <h2 className="text-[13px] font-semibold">
          {RAIL_TITLES[rail.kind] ?? rail.kind.replace(/-/g, " ")}
        </h2>
        <Button variant="ghost" size="icon-sm" onClick={close} aria-label="Close panel (Esc)">
          <X className="size-4" />
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
  if (kind === "draft-assist" && isDraftAssistPayload(payload)) {
    return <DraftAssistPanel threadId={payload.threadId} />;
  }
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
    const attachments = payload.filter(isAttachmentView);
    return (
      <div className="space-y-2">
        {attachments.map((attachment, index) => (
          <AttachmentActions key={attachment.id ?? index} attachment={attachment} />
        ))}
      </div>
    );
  }
  if (kind === "sender-profile") {
    return <SenderProfilePanel payload={payload} />;
  }
  if (kind === "commitments") {
    return <CommitmentsPanel payload={payload} />;
  }
  if (kind === "thread-briefing" || kind === "recipient-briefing") {
    return <BriefingPanel payload={payload} />;
  }
  if (kind === "expert-finder") {
    return <ExpertFinderPanel />;
  }
  if (kind === "whois" && isWhoisPayload(payload)) {
    return <WhoisPanel entity={payload.entity} accountId={payload.accountId} />;
  }
  if (kind === "ask-archive") {
    return <AskArchivePanel />;
  }
  return (
    <p className="py-8 text-center text-[13px] text-muted-foreground">Nothing to show here.</p>
  );
}

const RAIL_TITLES: Record<string, string> = {
  "draft-assist": "Draft a reply",
  "thread-context": "Context",
  attachments: "Attachments",
  "sender-profile": "Sender",
  commitments: "Commitments",
  "thread-briefing": "Thread briefing",
  "recipient-briefing": "Recipient briefing",
  "expert-finder": "Find an expert",
  whois: "Who is this?",
  "ask-archive": "Ask your archive",
};
