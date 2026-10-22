/*
 * Analytics-related palette actions.
 */

import { BarChart3, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { refreshAnalyticsContacts } from "@/features/analytics/api";
import type { ActionBehavior } from "@/lib/actions/types";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { useModals } from "@/state/modalStore";

function navigate(to: string): () => void {
  return () => {
    useModals.getState().setCommandPaletteOpen(false);
    getRuntimeNavigate().navigate(to);
  };
}

export const analyticsActions: ActionBehavior[] = [
  {
    id: "analytics.refresh-contacts",
    icon: RefreshCw,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      refreshAnalyticsContacts()
        .then(() => toast.success("Contact refresh queued"))
        .catch((error: Error) =>
          toast.error("Contact refresh failed", { description: error.message }),
        );
    },
  },
  { id: "analytics.wrapped", icon: BarChart3, run: navigate("/analytics/wrapped") },
];
