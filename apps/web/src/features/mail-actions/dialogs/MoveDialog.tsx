import { useQuery } from "@tanstack/react-query";
import { ArrowRightCircle, Route as RouteIcon } from "lucide-react";
import { useMemo, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { routeMessages } from "@/features/mailbox/api";
import { lensesFromShell } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { plural } from "@/lib/format";

import { performMailAction } from "../mailMutations";
import { describeTarget } from "../mailVerbs";
import type { MailTarget } from "../target";

/**
 * Pick one destination label. A plain move takes the mail out of where it
 * is (TUI `v`); a route also clears the current queue label, marks it read
 * and archives it (TUI "Route To Label"). Route sends the real label name.
 */
export function MoveDialog({
  target,
  route,
  onDone,
  onClose,
}: {
  target: MailTarget;
  route?: { fromQueueLabel: string };
  onDone?: () => void;
  onClose: () => void;
}) {
  const shell = useShellQuery();
  const destinations = useMemo(
    () =>
      lensesFromShell(shell.data)
        .filter((lens) => lens.section === "labels" && lens.label !== route?.fromQueueLabel)
        .map((lens) => lens.label),
    [route?.fromQueueLabel, shell.data],
  );

  // A batch route is previewed first: the daemon's dry run over the same
  // message ids says what will change before anything does.
  const [confirming, setConfirming] = useState<string | null>(null);
  const previewNeeded = Boolean(route) && target.messageIds.length > 1;

  const run = (label: string) => {
    onClose();
    onDone?.();
    if (route) {
      void performMailAction("route", target.messageIds, {
        payload: { label, fromQueueLabel: route.fromQueueLabel, archive: true },
      });
    } else {
      void performMailAction("move", target.messageIds, { payload: { label } });
    }
  };

  const choose = (label: string) => {
    if (previewNeeded) setConfirming(label);
    else run(label);
  };

  if (route && confirming) {
    return (
      <RoutePreview
        target={target}
        fromQueueLabel={route.fromQueueLabel}
        toLabel={confirming}
        onConfirm={() => run(confirming)}
        onBack={() => setConfirming(null)}
        onClose={onClose}
      />
    );
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md gap-0 overflow-hidden p-0">
        <div className="border-b border-border px-4 pb-3 pt-4">
          <DialogTitle>{route ? `Route out of ${route.fromQueueLabel}` : "Move to"}</DialogTitle>
          <DialogDescription className="mt-1 truncate">
            {route
              ? "Adds the label, clears the queue label, marks read and archives."
              : describeTarget(target)}
          </DialogDescription>
        </div>
        <Command loop>
          <CommandInput placeholder="Destination label…" />
          <CommandList className="max-h-72">
            <CommandEmpty>No label by that name.</CommandEmpty>
            <CommandGroup>
              {destinations.map((label) => (
                <CommandItem key={label} value={label} onSelect={() => choose(label)}>
                  {route ? (
                    <RouteIcon className="size-4" />
                  ) : (
                    <ArrowRightCircle className="size-4" />
                  )}
                  {label}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}

function RoutePreview({
  target,
  fromQueueLabel,
  toLabel,
  onConfirm,
  onBack,
  onClose,
}: {
  target: MailTarget;
  fromQueueLabel: string;
  toLabel: string;
  onConfirm: () => void;
  onBack: () => void;
  onClose: () => void;
}) {
  const preview = useQuery({
    queryKey: ["route-preview", toLabel, fromQueueLabel, target.messageIds],
    queryFn: () =>
      routeMessages({
        messageIds: target.messageIds,
        toLabel,
        fromQueueLabel,
        archive: true,
        dryRun: true,
      }),
    staleTime: 0,
  });
  const result = preview.data?.result;
  const accounts = result?.accounts ?? [];
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-md"
        onEscapeKeyDown={(event) => {
          // Escape steps back to the label list, not out of the dialog.
          event.preventDefault();
          onBack();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && result) {
            event.preventDefault();
            onConfirm();
          }
        }}
      >
        <DialogTitle>Route to {toLabel}?</DialogTitle>
        <DialogDescription>
          {preview.isPending
            ? "Checking what this will change…"
            : preview.isError
              ? `Couldn't preview the route: ${preview.error.message}`
              : `${plural(result?.requested ?? 0, "message")}${
                  accounts.length > 1 ? ` in ${accounts.length} accounts` : ""
                } will be labelled ${toLabel}, leave ${fromQueueLabel}, be marked read and archived.`}
        </DialogDescription>
        {accounts.length > 1 ? (
          <ul className="grid gap-1 text-[13px] text-muted-foreground">
            {accounts.map((account) => (
              <li key={account.account_id} className="flex justify-between">
                <span>{account.account_name}</span>
                <span className="font-mono tabular-nums">{account.skipped}</span>
              </li>
            ))}
          </ul>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onBack}>
            Back <KeyChip className="ml-1">Esc</KeyChip>
          </Button>
          <Button onClick={onConfirm} disabled={!result}>
            Route {plural(result?.requested ?? target.messageIds.length, "message")}
            <KeyChip className="ml-1 border-primary-foreground/30 bg-transparent text-primary-foreground/80">
              ↵
            </KeyChip>
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
