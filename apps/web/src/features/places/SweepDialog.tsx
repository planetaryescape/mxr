import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { plural } from "@/lib/format";

import { previewSweep, type SweepScope } from "./api";
import { bundleSender, sweepConfirmLabel, sweepNote, sweepTitle } from "./placeCopy";
import { runSweep } from "./sweep";

/**
 * The sweep preview. Its numbers are the daemon's dry run of the very
 * request the confirm button sends, so what you confirm is what runs.
 */
export function SweepDialog({
  scope,
  senderLabel,
  shownHere,
  onClose,
}: {
  scope: SweepScope;
  senderLabel?: string;
  shownHere?: number;
  onClose: () => void;
}) {
  const preview = useQuery({
    queryKey: ["sweep-preview", scope.place, scope.accountId ?? "all", scope.senderEmail ?? "all"],
    queryFn: () => previewSweep(scope),
    staleTime: 0,
    gcTime: 0,
  });
  const data = preview.data?.preview;
  const senders = data?.senders ?? [];
  // `A` sits next to `S`: a slip to the whole place followed by Enter must
  // not archive it, so that sweep opens on Cancel and takes Tab, Enter.
  const wholePlace = !scope.senderEmail;
  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent data-testid="sweep-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>
            {data ? sweepTitle(data, senderLabel) : "Counting what to sweep…"}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {preview.isError
              ? `Preview failed: ${preview.error.message}`
              : data
                ? sweepNote(data, shownHere)
                : "Asking the daemon for the exact selection."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : data && data.count > 0 ? (
          <div className="grid gap-3 text-[13px]">
            {!senderLabel && senders.length > 0 ? (
              <ul aria-label="Senders" className="grid gap-0.5">
                {senders.slice(0, 5).map((sender) => (
                  <li
                    key={`${sender.account_id}|${sender.sender_email}`}
                    className="flex items-baseline justify-between gap-4"
                  >
                    <span className="min-w-0 truncate">{bundleSender(sender)}</span>
                    <span className="font-mono text-2xs tabular-nums text-muted-foreground">
                      {sender.count}
                    </span>
                  </li>
                ))}
                {senders.length > 5 ? (
                  <li className="text-muted-foreground">
                    and {plural(senders.length - 5, "more sender")}
                  </li>
                ) : null}
              </ul>
            ) : null}
            <ul
              aria-label="Sample subjects"
              className="grid gap-0.5 border-l border-border pl-3 text-muted-foreground"
            >
              {data.sample_subjects.map((subject, index) => (
                // Subjects can repeat (a weekly receipt); position keeps them apart.
                // oxlint-disable-next-line react/no-array-index-key
                <li key={`${index}-${subject}`} className="truncate">
                  {subject || "(no subject)"}
                </li>
              ))}
              {data.count > data.sample_subjects.length ? (
                <li className="font-mono text-2xs">
                  and {plural(data.count - data.sample_subjects.length, "more")}
                </li>
              ) : null}
            </ul>
          </div>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel autoFocus={wholePlace}>
            {data?.count === 0 ? "Close" : "Cancel"}
          </AlertDialogCancel>
          {data && data.count > 0 ? (
            <AlertDialogAction
              autoFocus={!wholePlace}
              onClick={() => {
                onClose();
                void runSweep(scope, data, senderLabel);
              }}
            >
              {sweepConfirmLabel(data, wholePlace)}
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
