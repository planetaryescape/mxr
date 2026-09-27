import { Check } from "lucide-react";

import { KeyChip } from "@/components/KeyChip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

import type { MailKind } from "./api";
import { KIND_LABELS, KIND_OPTIONS, kindOptionForKey, type KindOption } from "./placeCopy";
import { correctSender } from "./senderKind";

/**
 * "Move sender to…": one key per place. The current kind is marked; when
 * the user chose it, Automatic is how to undo the choice for good.
 */
export function SenderKindDialog({
  accountId,
  senderEmail,
  senderLabel,
  current,
  onClose,
}: {
  accountId: string;
  senderEmail: string;
  senderLabel: string;
  current?: MailKind;
  onClose: () => void;
}) {
  const choose = (option: KindOption) => {
    onClose();
    void correctSender({ accountId, senderEmail, label: senderLabel }, option.kind);
  };
  const isCurrent = (option: KindOption) =>
    current
      ? option.kind === null
        ? !current.corrected
        : current.corrected && option.kind === current.kind
      : false;
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-sm"
        data-testid="sender-kind-dialog"
        onKeyDown={(event) => {
          if (event.metaKey || event.ctrlKey || event.altKey) return;
          const option = kindOptionForKey(event.key);
          if (!option) return;
          event.preventDefault();
          choose(option);
        }}
      >
        <DialogHeader>
          <DialogTitle>Move {senderLabel} to…</DialogTitle>
          <DialogDescription>
            {current ? `Now in ${KIND_LABELS[current.kind]}: ${current.reason}. ` : ""}Their future
            mail follows.
          </DialogDescription>
        </DialogHeader>
        <ul className="grid gap-1">
          {KIND_OPTIONS.map((option) => (
            <li key={option.key}>
              <button
                type="button"
                onClick={() => choose(option)}
                aria-current={isCurrent(option) ? "true" : undefined}
                className={cn(
                  "flex w-full items-center gap-3 rounded-md px-3 py-2 text-left hover:bg-accent",
                  isCurrent(option) && "bg-accent/60",
                )}
              >
                <span className="flex-1">
                  <span className="block text-[13px] font-medium">{option.label}</span>
                  <span className="block text-2xs text-muted-foreground">{option.hint}</span>
                </span>
                {isCurrent(option) ? (
                  <Check aria-label="Current" className="size-3.5 text-primary" />
                ) : null}
                <KeyChip>{option.key}</KeyChip>
              </button>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
