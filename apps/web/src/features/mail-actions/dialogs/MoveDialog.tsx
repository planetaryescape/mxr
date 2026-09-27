import { ArrowRightCircle, Route as RouteIcon } from "lucide-react";
import { useMemo } from "react";

import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { lensesFromShell } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";

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

  const choose = (label: string) => {
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
