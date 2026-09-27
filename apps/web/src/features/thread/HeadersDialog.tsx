import { useQuery } from "@tanstack/react-query";
import { Copy } from "lucide-react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface HeadersResponse {
  headers: [string, string][];
}

function fetchHeaders(messageId: string) {
  return apiFetch<HeadersResponse>(
    `/api/v1/mail/messages/${encodeURIComponent(messageId)}/headers`,
  );
}

/** Headers repeat (Received:), so each row gets a stable position key. */
function numbered(headers: [string, string][]) {
  const seen = new Map<string, number>();
  return headers.map(([name, value]) => {
    const count = (seen.get(name) ?? 0) + 1;
    seen.set(name, count);
    return { key: `${name}#${count}`, name, value };
  });
}

/** Raw RFC 5322 headers for one message (CLI `mxr cat --view headers`). */
export function HeadersDialog({ messageId, onClose }: { messageId: string; onClose: () => void }) {
  const headers = useQuery({
    queryKey: ["message-headers", messageId],
    queryFn: () => fetchHeaders(messageId),
  });
  const text = (headers.data?.headers ?? []).map(([name, value]) => `${name}: ${value}`).join("\n");
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[85dvh] max-w-3xl flex-col">
        <DialogHeader>
          <DialogTitle>Raw headers</DialogTitle>
          <DialogDescription className="font-mono text-2xs">{messageId}</DialogDescription>
        </DialogHeader>
        {headers.isLoading ? (
          <div className="h-48 animate-pulse rounded-md bg-muted/60" />
        ) : headers.isError ? (
          <p className="text-sm text-destructive">Couldn't load headers: {headers.error.message}</p>
        ) : (
          <pre
            tabIndex={0}
            className="min-h-0 flex-1 overflow-auto rounded-md border border-border bg-muted/30 p-3 font-mono text-[12px] leading-5 whitespace-pre-wrap break-all"
          >
            {numbered(headers.data?.headers ?? []).map(({ key, name, value }) => (
              <div key={key}>
                <span className="text-primary">{name}:</span> {value}
              </div>
            ))}
          </pre>
        )}
        <div className="flex justify-end">
          <Button
            variant="outline"
            size="sm"
            disabled={!text}
            onClick={() =>
              void navigator.clipboard.writeText(text).then(
                () => toast.success("Headers copied"),
                () => toast.error("Couldn't copy to the clipboard"),
              )
            }
          >
            <Copy className="size-3.5" /> Copy all
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
