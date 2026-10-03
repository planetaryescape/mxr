/*
 * The words of the let-go preview, from the daemon's dry run: how many
 * updates go, and how many threads stay because another mode holds them.
 */

import type { ModeDoneOutcome } from "@/features/modes/modeDone";
import { plural } from "@/lib/format";

import type { NowUpdatesCard } from "./api";

export function letGoPreview(card: NowUpdatesCard, items: readonly ModeDoneOutcome[]) {
  const going = items.filter((item) => !item.error);
  const kept = going.filter((item) => item.still_in?.includes("todo")).length;
  const archived = going.filter((item) => (item.archived ?? 0) > 0).length;
  const title = `Let go of ${plural(card.message_count, "update")} from ${plural(card.source_count, "source")}?`;
  const parts = [
    archived > 0
      ? `${plural(archived, "conversation")} ${archived === 1 ? "leaves" : "leave"} your inbox.`
      : "Nothing leaves your inbox.",
    kept > 0
      ? `${plural(kept, "conversation")} also in To do ${kept === 1 ? "stays" : "stay"} there.`
      : null,
    "Undo puts it back.",
  ];
  return {
    title,
    note: parts.filter(Boolean).join(" "),
    threadIds: going.map((item) => item.thread_id),
  };
}
