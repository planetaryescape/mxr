/*
 * Analytics-related palette actions.
 */

import { BarChart3, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { refreshAnalyticsContacts } from "@/features/analytics/api";
import type { Action } from "@/lib/actions/types";
import { getRuntimeNavigate } from "@/lib/actions/runtime";
import { useModals } from "@/state/modalStore";

function navigate(to: string): () => void {
  return () => {
    useModals.getState().setCommandPaletteOpen(false);
    getRuntimeNavigate().navigate(to);
  };
}

export const analyticsActions: Action[] = [
  {
    id: "analytics.refresh-contacts",
    label: "Refresh contacts",
    description: "Rebuild relationship + contact analytics",
    group: "Analytics",
    icon: RefreshCw,
    paletteOnly: true,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      refreshAnalyticsContacts()
        .then(() => toast.success("Contact refresh queued"))
        .catch((error: Error) =>
          toast.error("Contact refresh failed", { description: error.message }),
        );
    },
  },
  {
    id: "analytics.wrapped",
    label: "Open Wrapped",
    description: "View year-in-review summary",
    group: "Analytics",
    icon: BarChart3,
    paletteOnly: true,
    run: navigate("/analytics/wrapped"),
  },
];
