/*
 * Past conversations, as the shared Row: the title, where it stands, the
 * time, and a mark for one waiting on you. j and k move, Enter opens, d
 * deletes, Esc goes back.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { assistantToast } from "./assistantToast";

import { assistantKeys, deleteConversation, type AssistantSummary } from "./api";
import { onBareKey } from "./cardKeys";
import { useConversations } from "./useAssistant";
import { Row, RowVerb } from "@/components/Row";
import { formatRelative, plural } from "@/lib/format";

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

  const remove = async (id: string) => {
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
          onClose();
          return;
        }
        onBareKey((key) => {
          const current = list[litIndex];
          if (key === "j") setLit(Math.min(litIndex + 1, list.length - 1));
          else if (key === "k") setLit(Math.max(litIndex - 1, 0));
          else if (key === "Enter" && current) open(current.id);
          else if (key === "d" && current) void remove(current.id);
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
                  <span>{formatRelative(new Date(summary.updated_at * 1000))}</span>
                </>
              }
              verbs={
                <RowVerb
                  keys="d"
                  label="delete"
                  ariaLabel={`Delete "${summary.title}"`}
                  onClick={() => void remove(summary.id)}
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
    </div>
  );
}
