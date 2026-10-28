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
 * "Move sender to…": one key per place, on the `g` letters. The current kind is marked; when
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
          // j, k and the arrows move the highlight; Enter (the focused
          // button) runs it, as in every picker (blueprint 35).
          if (["j", "k", "ArrowDown", "ArrowUp"].includes(event.key)) {
            event.preventDefault();
            const options = [...event.currentTarget.querySelectorAll<HTMLElement>("li > button")];
            const at = options.indexOf(document.activeElement as HTMLElement);
            const delta = event.key === "j" || event.key === "ArrowDown" ? 1 : -1;
            const next = at < 0 ? 0 : Math.min(options.length - 1, Math.max(0, at + delta));
            options[next]?.focus();
            return;
          }
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
                  <span className="block text-sm font-medium">{option.label}</span>
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
