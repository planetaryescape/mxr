/*
 * Mail palette actions without a single-key binding: routing out of a
 * queue label, draft assist, expert finder and manual sync. Keyed mail
 * verbs live in features/mail-actions/verbActions.ts.
 */

import { PenLine, RefreshCw, Route as RouteIcon, UserSearch } from "lucide-react";
import { toast } from "sonner";

import { apiFetch, BridgeRequestError } from "@/api/client";
import type { components } from "@/api/generated";
import type { Action } from "@/lib/actions/types";
import { fetchBridgeHealth } from "@/lib/bridgeHealth";
import {
  buildProtocolUpdateSteps,
  evaluateProtocolCompatibility,
} from "@/lib/protocolCompatibility";
import { useModals } from "@/state/modalStore";

export async function syncNow(): Promise<void> {
  try {
    const health = await fetchBridgeHealth();
    const mismatch = evaluateProtocolCompatibility(health);
    if (mismatch) {
      toast.error("Update mxr to sync all accounts", {
        description: protocolMismatchDescription(),
      });
      return;
    }
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    toast.error("Couldn't verify mxr compatibility", {
      description: `mxr couldn't be reached, so sync wasn't started. (${reason})`,
    });
    return;
  }

  try {
    const response = await apiFetch<unknown>("/api/v1/mail/sync/all", { method: "POST", body: {} });
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
    if (error instanceof BridgeRequestError && (error.status === 404 || error.status === 405)) {
      toast.error("Update mxr to sync all accounts", {
        description: `This daemon doesn't support global Sync now yet. Update mxr: ${buildProtocolUpdateSteps().join(" · ")}`,
      });
      return;
    }
    if (
      error instanceof BridgeRequestError &&
      (error.status === 426 || error.code === "protocol_mismatch")
    ) {
      toast.error("Update mxr to sync all accounts", {
        description: protocolMismatchDescription(),
      });
      return;
    }
    toast.error("Sync failed to start", {
      description: error instanceof Error ? error.message : String(error),
    });
  }
}

function protocolMismatchDescription(): string {
  return `This version of mxr needs updating before it can sync all accounts. Update mxr: ${buildProtocolUpdateSteps().join(" · ")}`;
}

type SyncAccountResultView = Pick<
  components["schemas"]["SyncAccountResult"],
  "account_name" | "result"
> & { error?: string };

function syncResults(value: unknown): SyncAccountResultView[] {
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
    const result: SyncAccountResultView = {
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
    label: "Draft a reply",
    description: "Generate a reply body for this conversation",
    group: "Compose",
    icon: PenLine,
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
