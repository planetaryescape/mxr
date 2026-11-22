import { Shield } from "lucide-react";
import { toast } from "sonner";

import { getActiveQueryClient } from "@/lib/queryClient";
import type { ActionBehavior } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

import { backfillSemantic, reindexSemantic, semanticSnapshot, setSemanticEnabled } from "./api";

function semanticEnabledFromCache(): boolean {
  const client = getActiveQueryClient();
  if (!client) return false;
  const cached = client.getQueryData(["diagnostics", "semantic"]);
  const snapshot = semanticSnapshot(cached as Parameters<typeof semanticSnapshot>[0]);
  if (!snapshot || typeof snapshot !== "object") return false;
  return (snapshot as { enabled?: boolean }).enabled === true;
}

function closePalette(): void {
  useModals.getState().setCommandPaletteOpen(false);
}

export const diagnosticsActions: ActionBehavior[] = [
  {
    id: "semantic.backfill",
    icon: Shield,
    run: () => {
      closePalette();
      backfillSemantic()
        .then(() => toast.success("Semantic backfill queued"))
        .catch((error: Error) =>
          toast.error("Semantic backfill failed", { description: error.message }),
        );
    },
  },
  {
    id: "semantic.enable",
    icon: Shield,
    when: () => !semanticEnabledFromCache(),
    run: () => {
      closePalette();
      setSemanticEnabled(true)
        .then(() => toast.success("Semantic search enabled"))
        .catch((error: Error) =>
          toast.error("Semantic update failed", { description: error.message }),
        );
    },
  },
  {
    id: "semantic.disable",
    icon: Shield,
    when: () => semanticEnabledFromCache(),
    run: () => {
      closePalette();
      setSemanticEnabled(false)
        .then(() => toast.success("Semantic search disabled"))
        .catch((error: Error) =>
          toast.error("Semantic update failed", { description: error.message }),
        );
    },
  },
  {
    id: "semantic.reindex",
    icon: Shield,
    run: () => {
      closePalette();
      reindexSemantic()
        .then(() => toast.success("Semantic reindex queued"))
        .catch((error: Error) =>
          toast.error("Semantic reindex failed", { description: error.message }),
        );
    },
  },
];
