import { Download, ExternalLink, Paperclip } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { downloadAttachment, openAttachment } from "@/features/mailbox/api";
import type { AttachmentView } from "@/features/mailbox/types";
import { cn, formatBytes } from "@/lib/utils";

interface AttachmentActionsProps {
  attachment: AttachmentView;
  messageId?: string;
  className?: string;
}

export function AttachmentActions({ attachment, messageId, className }: AttachmentActionsProps) {
  const [pending, setPending] = useState<"open" | "download" | null>(null);
  const filename = attachment.filename || "attachment";
  const resolvedMessageId = messageId ?? attachment.message_id;
  const canAct = Boolean(resolvedMessageId && attachment.id);

  async function run(action: "open" | "download") {
    if (!resolvedMessageId || !attachment.id) return;
    setPending(action);
    try {
      const result =
        action === "open"
          ? await openAttachment({ messageId: resolvedMessageId, attachmentId: attachment.id })
          : await downloadAttachment({ messageId: resolvedMessageId, attachmentId: attachment.id });
      toast.success(action === "open" ? "Opening attachment" : "Attachment downloaded", {
        description: result.file ?? filename,
      });
    } catch (error) {
      toast.error(action === "open" ? "Open failed" : "Download failed", {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setPending(null);
    }
  }

  return (
    <div
      data-testid="attachment-actions"
      className={cn(
        "flex min-w-0 flex-wrap items-center gap-2 rounded-lg border border-border bg-card px-3 py-2 text-xs",
        className,
      )}
    >
      <Paperclip className="size-3 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-foreground">{filename}</div>
        <div className="font-mono text-2xs text-muted-foreground">
          {attachment.mime_type || "unknown"} · {formatBytes(attachment.size_bytes ?? 0)}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          type="button"
          variant="secondary"
          size="xs"
          disabled={!canAct || pending !== null}
          onClick={() => void run("open")}
          aria-label={`Open ${filename}`}
        >
          <ExternalLink className="size-3" />
          Open
        </Button>
        <Button
          type="button"
          variant="outline"
          size="xs"
          disabled={!canAct || pending !== null}
          onClick={() => void run("download")}
          aria-label={`Download ${filename}`}
        >
          <Download className="size-3" />
          Download
        </Button>
      </div>
    </div>
  );
}
