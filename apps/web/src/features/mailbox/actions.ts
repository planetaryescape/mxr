/*
 * Mail palette actions without a single-key binding: routing out of a
 * queue label, draft assist, expert finder and manual sync. Keyed mail
 * verbs live in features/mail-actions/verbActions.ts.
 */

import { RefreshCw, Route as RouteIcon, Sparkles, UserSearch } from "lucide-react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import type { Action } from "@/lib/actions/types";
import { useConnectionStore } from "@/state/connectionStore";
import { useModals } from "@/state/modalStore";

export async function syncNow(): Promise<void> {
  if (useConnectionStore.getState().syncProgress) {
    toast.info("A sync is already running");
    return;
  }
  try {
    await apiFetch<unknown>("/api/v1/mail/sync", { method: "POST", body: {} });
    toast.success("Sync started", { description: "Progress shows in the status bar" });
  } catch (error) {
    toast.error("Sync failed to start", {
      description: error instanceof Error ? error.message : String(error),
    });
  }
}

export const mailboxActions: Action[] = [
  {
    id: "mail.route",
    command: "route",
    label: "Route out of this queue…",
    description: "Apply a label, clear the current queue label, mark read and archive",
    group: "Mail",
    icon: RouteIcon,
    scopes: ["list", "reader"],
    paletteOnly: true,
  },
  {
    id: "mail.draft-assist",
    command: "draftAssist",
    label: "Draft a reply with AI",
    description: "Generate a reply body for this conversation",
    group: "Compose",
    icon: Sparkles,
    scopes: ["reader"],
    paletteOnly: true,
  },
  {
    id: "mail.find-expert",
    label: "Find an expert",
    description: "People in your archive who answered similar questions",
    group: "Mail",
    icon: UserSearch,
    paletteOnly: true,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      useModals.getState().openRightRail("expert-finder");
    },
  },
  {
    id: "mail.sync",
    label: "Sync now",
    description: "Fetch new mail from every account",
    group: "Mail",
    icon: RefreshCw,
    paletteOnly: true,
    run: () => void syncNow(),
  },
];
