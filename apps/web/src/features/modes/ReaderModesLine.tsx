import { useRouterState } from "@tanstack/react-router";

import { AlsoInLine } from "./AlsoInLine";
import { useThreadModes, type ModeKind } from "./membership";

/** The mode a path belongs to; Now, Inbox and the rest belong to none. */
export function modeOfPath(path: string): ModeKind | null {
  const first = path.split("/").find(Boolean);
  switch (first) {
    case "messages":
    case "desk":
      return "messages";
    case "todo":
      return "todo";
    case "updates":
      return "updates";
    case "reading":
      return "reading";
    case "archive":
      return "archive";
    default:
      return null;
  }
}

/**
 * Under the reader's header: "Also in To do: sign by Wed 15 Oct", the
 * other modes holding this conversation, from `GetModeMembership`. In the
 * Inbox, a lens over every mode, it names every mode that holds it.
 */
export function ReaderModesLine({ threadId }: { threadId: string }) {
  const path = useRouterState({ select: (state) => state.location.pathname });
  const modes = useThreadModes(threadId).data;
  return (
    <AlsoInLine
      modes={modes}
      here={modeOfPath(path)}
      className="shrink-0 border-b border-border px-5 py-1.5"
    />
  );
}
