/*
 * Where you write to the assistant. Enter sends, Shift+Enter adds a line,
 * Esc leaves the field (to a waiting proof, if there is one, so its keys
 * can answer). The model and where it runs sit under the field, once.
 */

import { useEffect, useRef, useState, type FormEvent } from "react";

import { ASSISTANT_SHORTCUT } from "./actions";
import { useAssistantUi } from "./assistantStore";
import { PENDING_CARD_ATTRIBUTE } from "./cardKeys";
import { useAvailability } from "./useAssistant";
import { KeyChip } from "@/components/KeyChip";
import { formatChord } from "@/lib/keys/chord";
import { cn } from "@/lib/utils";

export function Composer({
  disabled,
  running,
  waiting,
  onSend,
}: {
  disabled: boolean;
  running: boolean;
  waiting: boolean;
  onSend: (text: string) => void;
}) {
  const [text, setText] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const focusRequest = useAssistantUi((s) => s.focusRequest);

  useEffect(() => {
    if (focusRequest === 0) return;
    // After the pane's open transition has placed it.
    const frame = requestAnimationFrame(() => input.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [focusRequest]);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (!text.trim() || running || disabled) return;
    onSend(text);
    setText("");
  };

  return (
    <form
      onSubmit={submit}
      className={cn(
        "border border-rule bg-ink-room focus-within:border-signal",
        disabled && "opacity-60",
      )}
    >
      <textarea
        ref={input}
        aria-label="Message the assistant"
        value={text}
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          // Enter sends; Shift+Enter adds a line.
          if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault();
            submit();
          }
          if (event.key === "Escape") {
            // A waiting proof takes the keyboard, so `y` and `n` can answer it.
            const card = waiting
              ? document.querySelector<HTMLElement>(`[${PENDING_CARD_ATTRIBUTE}]`)
              : null;
            if (card) card.focus();
            else input.current?.blur();
          }
        }}
        placeholder={
          waiting
            ? "Esc to answer above, or write to move on"
            : "Ask about your mail, or say what to do"
        }
        rows={2}
        className="type-body block max-h-48 min-h-14 w-full resize-none bg-transparent px-3 pt-2.5 pb-1 text-text-1 outline-none placeholder:text-text-3 disabled:cursor-not-allowed"
      />
      <div className="type-meta flex items-center gap-2 px-3 pb-2 text-text-3">
        <span className="min-w-0 flex-1 truncate">
          {waiting && text.trim() ? "Sending this declines what is waiting above." : <Disclosure />}
        </span>
        <span className="flex shrink-0 items-center gap-1 pointer-coarse:hidden">
          <KeyChip className="h-4 px-1">↵</KeyChip>
          <span>send</span>
          <KeyChip className="ml-1 h-4 px-1">{formatChord(ASSISTANT_SHORTCUT)}</KeyChip>
          <span>hide</span>
        </span>
      </div>
    </form>
  );
}

/** Where the conversation goes, said once, inside the composer. */
function Disclosure() {
  const availability = useAvailability();
  const data = availability.data;
  if (!data?.available || !data.model) return null;
  return (
    <span
      title={
        data.leaves_machine
          ? "Your messages and the mail it reads go to this provider"
          : "Nothing leaves this machine"
      }
    >
      {data.model} · {data.leaves_machine ? "sent off this machine" : "runs on this machine"}
    </span>
  );
}
