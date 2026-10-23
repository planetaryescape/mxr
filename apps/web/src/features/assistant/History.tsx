/*
 * Past conversations, as the shared Row: the title, where it stands, the
 * time, and a mark for one waiting on you. j and k move, Enter opens, Esc
 * goes back. # asks the daemon what would go (a dry run), then y or Enter
 * deletes and n or Esc keeps it, as in the TUI. d deleted until K3
 * (blueprint 35) and says so until 0.6.185, as the definition's moved keys do.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { assistantToast } from "./assistantToast";

import {
  assistantKeys,
  deleteConversation,
  type AssistantDelete,
  type AssistantSummary,
} from "./api";
import { onBareKey } from "./cardKeys";
import { QuietKey } from "./Proof";
import { useConversations } from "./useAssistant";
import { KeyChip } from "@/components/KeyChip";
import { Row, RowVerb } from "@/components/Row";
import { Button } from "@/components/ui/button";
import { formatRelative, plural } from "@/lib/format";
import { useKeyScope } from "@/state/keyScopeStore";

function standing(summary: AssistantSummary): string {
  if (summary.pending_count > 0) return "Waiting for you";
  if (summary.status === "running") return "Working";
  return plural(summary.message_count, "message");
}

export function History({
  currentId,
  onPick,
  onClose,
}: {
  currentId: string | null;
  onPick: (id: string | null) => void;
  onClose: () => void;
}) {
  const conversations = useConversations(true);
  const queryClient = useQueryClient();
  const list = conversations.data ?? [];
  const [lit, setLit] = useState(0);
  const litIndex = Math.min(lit, Math.max(0, list.length - 1));
  const [confirming, setConfirming] = useState<AssistantDelete | null>(null);

  const askToRemove = async (id: string) => {
    try {
      setConfirming(await deleteConversation(id, { dryRun: true }));
    } catch (error) {
      assistantToast.error(error instanceof Error ? error.message : String(error));
    }
  };
  const remove = async (id: string) => {
    setConfirming(null);
    try {
      await deleteConversation(id);
      if (id === currentId) onPick(null);
      void queryClient.invalidateQueries({ queryKey: assistantKeys.list });
      queryClient.removeQueries({ queryKey: assistantKeys.conversation(id) });
    } catch (error) {
      assistantToast.error(error instanceof Error ? error.message : String(error));
    }
  };
  const open = (id: string) => {
    onPick(id);
    onClose();
  };

  return (
    <div
      tabIndex={-1}
      data-keys-owner=""
      ref={(element) => element?.focus({ preventScroll: true })}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          if (confirming) setConfirming(null);
          else onClose();
          return;
        }
        onBareKey((key, keyEvent) => {
          if (confirming) {
            // Enter on a focused button keeps its own meaning ("Keep" must not delete).
            const onButton = keyEvent.target !== keyEvent.currentTarget;
            if (key === "y" || (key === "Enter" && !onButton)) {
              void remove(confirming.conversation_id);
            } else if (key === "n") setConfirming(null);
            else return false;
            return true;
          }
          const current = list[litIndex];
          if (key === "j") setLit(Math.min(litIndex + 1, list.length - 1));
          else if (key === "k") setLit(Math.max(litIndex - 1, 0));
          else if (key === "Enter" && current) open(current.id);
          else if (key === "#" && current) void askToRemove(current.id);
          else if (key === "d") useKeyScope.getState().showMoved("Delete", "#");
          else return false;
          return true;
        })(event);
      }}
      className="-mx-2 outline-none"
    >
      <h3 className="type-eyebrow px-3 pb-2 text-text-3">Past conversations</h3>
      {conversations.isPending ? (
        <p className="type-meta px-3 text-text-3">Loading…</p>
      ) : list.length === 0 ? (
        <p className="type-body px-3 text-text-2">None yet.</p>
      ) : (
        <ul aria-label="Past conversations">
          {list.map((summary, index) => (
            <Row
              key={summary.id}
              lit={index === litIndex}
              via="pointer"
              onMouseEnter={() => setLit(index)}
              primary={summary.title}
              secondary={standing(summary)}
              trail={
                <>
                  {summary.pending_count > 0 ? (
                    <span className="text-cue" role="img" aria-label="Waiting for you">
                      ◇
                    </span>
                  ) : null}
                </>
              }
              time={<span>{formatRelative(new Date(summary.updated_at * 1000))}</span>}
              verbs={
                <RowVerb
                  keys="#"
                  label="delete"
                  ariaLabel={`Delete "${summary.title}"`}
                  onClick={() => void askToRemove(summary.id)}
                />
              }
              renderMain={(content, className) => (
                <button type="button" onClick={() => open(summary.id)} className={className}>
                  {content}
                </button>
              )}
            />
          ))}
        </ul>
      )}
      {confirming ? (
        <div className="mt-2 border-t border-rule-soft px-3 pt-2">
          <p role="status" className="type-body text-danger">
            Delete "{confirming.title}" and its {plural(confirming.message_count, "message")}? This
            can't be undone.
          </p>
          <div className="mt-2 flex flex-wrap items-center gap-x-1 gap-y-2">
            <QuietKey keys="n" label="keep" onClick={() => setConfirming(null)} />
            <Button
              size="sm"
              variant="destructive"
              className="ml-auto"
              onClick={() => void remove(confirming.conversation_id)}
            >
              Delete conversation
              <KeyChip className="pointer-coarse:hidden border-transparent bg-transparent text-inherit shadow-none">
                y
              </KeyChip>
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
