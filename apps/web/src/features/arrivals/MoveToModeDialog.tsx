import { KeyChip } from "@/components/KeyChip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

import { performMove, type MoveSubject } from "./moves";
import { MODE_NAMES, modeForKey, MOVE_KEYS, SENDER_MODES, type ModeId } from "./types";

/** What one key press in the picker asks for, or nothing. */
export function pickerChoice(
  key: string,
  sender: boolean,
): { mode: ModeId; sender: boolean } | null {
  const lower = key.toLowerCase();
  const mode = modeForKey(lower);
  if (!mode || key.length !== 1) return null;
  // A capital in the email picker means "for this sender"; the sender
  // picker takes either case.
  const forSender = sender || key !== lower;
  if (forSender && !SENDER_MODES.includes(mode)) return null;
  return { mode, sender: forSender };
}

/**
 * "Move to…": one key per mode. `X` moves this email; `K` (or a capital in
 * this picker) sends everything from its sender, which only Messages,
 * Updates and Reading can take.
 */
export function MoveToModeDialog({
  subject,
  sender,
  onClose,
}: {
  subject: MoveSubject;
  sender: boolean;
  onClose: () => void;
}) {
  const choose = (mode: ModeId, forSender: boolean) => {
    onClose();
    void performMove({ messageId: subject.messageId, mode, sender: forSender });
  };
  const modes = Object.entries(MOVE_KEYS).filter(
    ([, mode]) => !sender || SENDER_MODES.includes(mode),
  );
  const who = subject.senderLabel ?? "this sender";
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-sm"
        data-testid="move-to-mode-dialog"
        onKeyDown={(event) => {
          if (event.metaKey || event.ctrlKey || event.altKey) return;
          const choice = pickerChoice(event.key, sender);
          if (!choice) return;
          event.preventDefault();
          choose(choice.mode, choice.sender);
        }}
      >
        <DialogHeader>
          <DialogTitle>
            {sender ? `Send everything from ${who} to…` : `Move "${subject.label}" to…`}
          </DialogTitle>
          <DialogDescription>
            {sender
              ? "Their mail goes there from now on."
              : `This email only. Shift with the key sends everything from ${who}.`}
          </DialogDescription>
        </DialogHeader>
        <ul className="grid gap-1">
          {modes.map(([key, mode]) => (
            <li key={mode}>
              <button
                type="button"
                data-testid={`move-choice-${mode}`}
                onClick={() => choose(mode, sender)}
                className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-[13px] hover:bg-accent"
              >
                <span className="flex-1 font-medium">{MODE_NAMES[mode]}</span>
                <KeyChip>{sender ? key.toUpperCase() : key}</KeyChip>
              </button>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
