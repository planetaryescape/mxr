import { useRouterState } from "@tanstack/react-router";

import { AlsoInLine } from "./AlsoInLine";
import { useThreadModes, type ModeKind } from "./membership";

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

export function ReaderModesLine({ threadId }: { threadId: string }) {
  const path = useRouterState({ select: (state) => state.location.pathname });
  const modes = useThreadModes(threadId).data;
  return (
    <AlsoInLine
      modes={modes}
      here={modeOfPath(path)}
      className="reader-band shrink-0 border-b border-border py-1.5"
    />
  );
}
