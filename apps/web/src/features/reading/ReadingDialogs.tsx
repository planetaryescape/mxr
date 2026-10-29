import { useQuery } from "@tanstack/react-query";
import { Archive, Loader2, MailX } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { commitUnsubscribePreview, unsubscribeAndClearSender } from "@/features/mailbox/api";
import { threadAccounts, conversationKey, type ThreadRef } from "@/features/mailbox/rowKey";
import { doneModeRequest } from "@/features/modes/modeDone";
import { plural } from "@/lib/format";

import type { ReadingSource } from "./api";
import { unsubscribeMethodLine, unsubscribePreview } from "./readingView";
import { letGo, refreshReading } from "./readingVerbs";
import { announceMovedOverlayKey, optionLetter, pressedOverlayKey } from "@/lib/keys/overlayKeys";

const IRREVERSIBLE = "This can't be undone from mxr; you'd resubscribe on their site.";

/**
 * `D`: unsubscribe, with the evidence first. Two choices: just unsubscribe
 * (Enter or u, keeps the mail already there) or unsubscribe and clear the
 * sender's issues (a). The daemon's dry run counts the mail the second
 * choice would clear, and nothing is sent until one of the buttons is
 * pressed.
 */
export function ReadingUnsubscribeDialog({
  source,
  onClose,
}: {
  source: ReadingSource;
  onClose: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const preview = useQuery({
    queryKey: ["reading", "unsubscribe-preview", source.account_id, source.sender_email],
    queryFn: () =>
      unsubscribeAndClearSender({
        address: source.sender_email,
        accountId: source.account_id,
        dryRun: true,
      }),
    staleTime: 0,
  });
  // Only a finished preview with a token and a method can be committed,
  // and the method shown is the one in that preview.
  const ready = preview.isFetching ? null : unsubscribePreview(preview);
  const count = ready?.count ?? preview.data?.result?.message_count;

  async function commitUnsubscribe(archive: boolean) {
    if (!ready || busy) return;
    setBusy(true);
    try {
      const answer = await commitUnsubscribePreview({
        address: source.sender_email,
        accountId: source.account_id,
        previewToken: ready.token,
        archive,
      });
      const result = answer.result;
      if (result?.error) {
        toast.error(`Couldn't unsubscribe from ${source.name}`, { description: result.error });
      } else {
        toast.success(
          archive && result && result.archived_count > 0
            ? `Unsubscribed from ${source.name} and let go of ${plural(result.archived_count, "issue")}`
            : `Unsubscribed from ${source.name}`,
        );
      }
      await refreshReading();
      onClose();
    } catch (error) {
      toast.error(`Couldn't unsubscribe from ${source.name}`, {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setBusy(false);
    }
  }

  const clearLabel =
    count != null
      ? `Unsubscribe and clear ${plural(count, "issue")}`
      : "Unsubscribe and clear its issues";

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-md grid-cols-[minmax(0,1fr)]"
        data-testid="reading-unsubscribe"
        onKeyDown={(event) => {
          // Enter on a focused button is that button's click, not a shortcut.
          if (event.key === "Enter" && event.target instanceof HTMLButtonElement) return;
          if (pressedOverlayKey(event, "unsubscribe.confirm")) {
            event.preventDefault();
            void commitUnsubscribe(false);
          } else if (pressedOverlayKey(event, "unsubscribe.and-archive")) {
            event.preventDefault();
            void commitUnsubscribe(true);
          } else if (pressedOverlayKey(event, "overlay.no")) {
            // n is Esc, as in the TUI (blueprint 35).
            event.preventDefault();
            onClose();
          } else if (announceMovedOverlayKey(event, "unsubscribe")) {
            event.preventDefault();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>Unsubscribe from {source.name}?</DialogTitle>
          <DialogDescription data-testid="unsubscribe-evidence">
            {source.evidence}.
          </DialogDescription>
        </DialogHeader>
        <ul className="grid gap-1.5 text-sm text-foreground/90">
          <li data-testid="unsubscribe-method">
            {preview.isLoading
              ? "Checking how to unsubscribe…"
              : preview.isError
                ? `Couldn't check: ${preview.error instanceof Error ? preview.error.message : String(preview.error)}`
                : unsubscribeMethodLine(ready?.method ?? "none")}
          </li>
          <li data-testid="unsubscribe-count">
            {preview.isLoading ? (
              <span className="inline-flex items-center gap-1.5 text-muted-foreground">
                <Loader2 className="size-3.5 animate-spin" /> Counting its mail…
              </span>
            ) : count != null ? (
              `Also lets go of ${plural(count, "issue")} from it in your inbox.`
            ) : (
              "Its issues in your inbox are let go of too."
            )}
          </li>
          <li data-testid="unsubscribe-irreversible" className="text-muted-foreground">
            {IRREVERSIBLE}
          </li>
        </ul>
        <div className="grid gap-2">
          <button
            type="button"
            data-testid="unsubscribe-just"
            onClick={() => void commitUnsubscribe(false)}
            disabled={busy || !ready}
            className="flex items-center gap-3 rounded-md border border-border px-3 py-2.5 text-left hover:border-primary/60 hover:bg-accent disabled:opacity-60"
          >
            <MailX className="size-4 text-muted-foreground" />
            <span className="flex-1">
              <span className="block text-sm font-medium">
                Just unsubscribe — keep what you have
              </span>
            </span>
            <KeyChip>{optionLetter("unsubscribe.confirm")}</KeyChip>
          </button>
          <button
            type="button"
            data-testid="unsubscribe-clear"
            onClick={() => void commitUnsubscribe(true)}
            disabled={busy || !ready}
            className="flex items-center gap-3 rounded-md border border-border px-3 py-2.5 text-left hover:border-primary/60 hover:bg-accent disabled:opacity-60"
          >
            {preview.isLoading ? (
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            ) : (
              <Archive className="size-4 text-muted-foreground" />
            )}
            <span className="flex-1">
              <span className="block text-sm font-medium">{clearLabel}</span>
              <span className="block text-2xs text-muted-foreground">
                Marks them read and archives them
              </span>
            </span>
            <KeyChip>{optionLetter("unsubscribe.and-archive")}</KeyChip>
          </button>
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            Keep it
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/**
 * `A`: let go of everything the edition shows. The daemon's dry run lists
 * exactly the conversations; confirming sends those same ids.
 */
export function LetGoAllDialog({
  threads,
  titles,
  onClose,
}: {
  threads: readonly ThreadRef[];
  titles: readonly string[];
  onClose: () => void;
}) {
  const preview = useQuery({
    queryKey: [
      "reading",
      "let-go-all",
      ...threads.map((t) => conversationKey(t.thread_id, t.account_id)),
    ],
    // The same pairs the confirm sends, so the preview and the mutation name the same conversations.
    queryFn: () =>
      doneModeRequest(
        "reading",
        threads.map((t) => t.thread_id),
        true,
        { accounts: threadAccounts(threads) },
      ),
    staleTime: 0,
  });
  const items = preview.data?.items ?? [];
  const ready = items.filter((item) => !item.error);
  const archived = ready.filter((item) => (item.archived ?? 0) > 0).length;
  const provider = ready.find((item) => (item.archived ?? 0) > 0)?.provider ?? "your mail";
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-lg grid-cols-[minmax(0,1fr)]"
        data-testid="reading-let-go-all"
      >
        <DialogHeader>
          <DialogTitle>Let go of {plural(threads.length, "item")}?</DialogTitle>
          <DialogDescription>
            They leave Reading. Anything on Later stays there.
            {archived > 0
              ? ` ${plural(archived, "conversation")} no other mode holds will be archived in ${provider} too.`
              : ""}
          </DialogDescription>
        </DialogHeader>
        {preview.isLoading ? (
          <p className="inline-flex items-center gap-1.5 text-sm text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" /> Checking what this lets go of…
          </p>
        ) : (
          <ul
            data-testid="let-go-all-preview"
            className="max-h-64 overflow-y-auto text-sm text-foreground/90"
          >
            {threads.map((thread, index) => {
              const outcome = items.find(
                (item) =>
                  item.thread_id === thread.thread_id &&
                  (!item.account_id || !thread.account_id || item.account_id === thread.account_id),
              );
              return (
                <li
                  key={conversationKey(thread.thread_id, thread.account_id)}
                  data-thread={thread.thread_id}
                  className="truncate py-0.5"
                >
                  {titles[index]}
                  {outcome?.error ? (
                    <span className="text-muted-foreground"> ({outcome.error})</span>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button
            size="sm"
            disabled={preview.isLoading || ready.length === 0}
            onClick={() => {
              onClose();
              void letGo(
                ready.map((item) => ({ thread_id: item.thread_id, account_id: item.account_id })),
              );
            }}
          >
            Let go of {plural(ready.length, "item")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
