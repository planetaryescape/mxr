/*
 * Reply where reading ends: a quiet field after the last message. Clicking
 * or focusing it opens the inline composer in its place (the same command
 * as `r`); reply all and forward sit below it with their keys, and "Draft
 * in your voice" opens the reply with the composer's "Draft for me". Hidden
 * while the inline composer is open, since the composer is then right here.
 */

import { Sparkles } from "lucide-react";

import { KeyChip } from "@/components/KeyChip";
import { useComposeUi } from "@/features/compose/composeUiStore";
import { runCommand } from "@/lib/keys/controllers";

const run = (command: string) => () => runCommand("reader", command);

export function ReplyField({
  name,
  canReplyAll,
  onDraftInVoice,
}: {
  /** Who a reply goes to, e.g. "Maya". */
  name: string | null;
  canReplyAll: boolean;
  /** Opens the reply with "Draft for me" expanded; only when a model is configured. */
  onDraftInVoice?: () => void;
}) {
  const composing = useComposeUi((s) => s.intent !== null && s.surface === "inline");
  if (composing) return null;
  return (
    <div className="px-5 pt-6" data-testid="reply-field">
      <div className="flex items-center gap-2 rounded-lg border border-border bg-surface/60 pr-2 transition-colors duration-fast hover:border-border-strong focus-within:border-primary/60">
        <button
          type="button"
          onClick={run("reply")}
          onFocus={run("reply")}
          className="min-w-0 flex-1 truncate px-4 py-3 text-left text-[13.5px] text-muted-foreground outline-none"
        >
          {name ? `Reply to ${name}…` : "Reply…"}
        </button>
        {onDraftInVoice ? (
          <button
            type="button"
            onClick={onDraftInVoice}
            aria-label="Draft in your voice"
            title="Draft in your voice"
            className="inline-flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <Sparkles className="size-3.5" aria-hidden />
            {/* Icon only in a narrow reader, so the name of who you reply to fits. */}
            <span className="hidden @md:inline">Draft in your voice</span>
          </button>
        ) : null}
        <KeyChip className="shrink-0">r</KeyChip>
      </div>
      <p className="mt-2 flex gap-4 pl-1 text-[12px] text-muted-foreground">
        {canReplyAll ? (
          <button
            type="button"
            onClick={run("replyAll")}
            className="inline-flex items-center gap-1.5 hover:text-foreground"
          >
            Reply all <KeyChip className="h-4 px-1">a</KeyChip>
          </button>
        ) : null}
        <button
          type="button"
          onClick={run("forward")}
          className="inline-flex items-center gap-1.5 hover:text-foreground"
        >
          Forward <KeyChip className="h-4 px-1">f</KeyChip>
        </button>
      </p>
    </div>
  );
}
