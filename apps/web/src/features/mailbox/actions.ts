/*
 * Mail palette actions without a single-key binding: routing out of a
 * queue label, draft assist, expert finder and manual sync. Keyed mail
 * verbs live in features/mail-actions/verbActions.ts.
 */

import { RefreshCw, Route as RouteIcon, Sparkles, UserSearch } from "lucide-react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import type { Action } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

export async function syncNow(): Promise<void> {
  try {
    const response = await apiFetch<unknown>("/api/v1/mail/sync", { method: "POST", body: {} });
    const results = syncResults(response);
    if (results.length === 0) {
      toast.info("No enabled sync-capable accounts");
      return;
    }
    const failures = results.filter((result) => result.result === "failed");
    if (failures.length > 0) {
      toast.error("Some accounts couldn't sync", {
        description: failures
          .map((failure) => `${failure.account_name}: ${failure.error ?? "Sync couldn't start"}`)
          .join("; "),
      });
    }
  } catch (error) {
    toast.error("Sync failed to start", {
      description: error instanceof Error ? error.message : String(error),
    });
  }
}

function syncResults(value: unknown): Array<{
  account_name: string;
  result: "started" | "joined" | "failed";
  error?: string;
}> {
  if (!isRecord(value) || !Array.isArray(value.results)) {
    throw new Error("The daemon returned an invalid sync result");
  }
  return value.results.map((item) => {
    if (!isRecord(item)) {
      throw new Error("The daemon returned an invalid account sync result");
    }
    if (
      typeof item.account_name !== "string" ||
      (item.result !== "started" && item.result !== "joined" && item.result !== "failed") ||
      (item.error !== undefined && item.error !== null && typeof item.error !== "string")
    ) {
      throw new Error("The daemon returned an invalid account sync result");
    }
    const result: {
      account_name: string;
      result: "started" | "joined" | "failed";
      error?: string;
    } = {
      account_name: item.account_name,
      result: item.result,
    };
    if (typeof item.error === "string") result.error = item.error;
    return result;
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
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
