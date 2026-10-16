import { useCallback, useState } from "react";

import type { LampVia } from "@/components/Row";

/**
 * The lit row of a tool page's list (Invites, Deliveries, Drafts, Subscriptions).
 * These pages are worked with the pointer and Tab, not a list cursor: the
 * first row starts lit, and pressing or focusing inside a row moves the lamp
 * to it, so its verbs are one tap away on a phone and always reachable by Tab.
 */
export function useLitRow(ids: readonly string[]) {
  const [picked, setPicked] = useState<string | null>(null);
  const [via, setVia] = useState<LampVia>("key");
  const litId = picked !== null && ids.includes(picked) ? picked : (ids[0] ?? null);
  const rowProps = useCallback(
    (id: string) => ({
      lit: id === litId,
      via,
      onPointerDownCapture: () => {
        setVia("pointer");
        setPicked(id);
      },
      onFocusCapture: () => {
        setPicked(id);
      },
    }),
    [litId, via],
  );
  return { rowProps };
}

export type LitRowProps = ReturnType<ReturnType<typeof useLitRow>["rowProps"]>;
