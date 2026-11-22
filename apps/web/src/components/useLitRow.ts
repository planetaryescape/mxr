import { useCallback, useState } from "react";

import type { LampVia } from "@/components/Row";

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
