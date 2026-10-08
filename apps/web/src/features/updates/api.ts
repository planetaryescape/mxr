/*
 * Updates over the bridge. The daemon builds the digest: the cut, the
 * sections, every line's fact, numbers and delta, and the let-go selection
 * (`GetUpdatesDigest`); letting go and tuning answer with what they did.
 * This file only moves them.
 */

import { useQuery } from "@tanstack/react-query";
import { useRef } from "react";

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Schemas = components["schemas"];
export type UpdatesDigest = Schemas["UpdatesDigestData"];
export type UpdateLine = Schemas["UpdateLineData"];
export type UpdateLink = Schemas["UpdateLinkData"];
export type UpdateSection = Schemas["UpdateSectionData"];
export type UpdateSetting = Schemas["UpdateSourceSettingData"];
export type UpdatesLetGo = Schemas["UpdatesLetGoData"];
export type UpdateSourceChange = Schemas["UpdateSourceChangeData"];

type Digest = Extract<Schemas["ResponseData"], { kind: "UpdatesDigest" }>;
type LetGo = Extract<Schemas["ResponseData"], { kind: "UpdatesLetGo" }>;
type Source = Extract<Schemas["ResponseData"], { kind: "UpdateSource" }>;

/**
 * The bridge passes any daemon response through with a 200, so a reply of
 * another kind would otherwise surface later as a missing field in render.
 */
function expectKind<T extends { kind: string }>(answer: { kind?: unknown }, kind: T["kind"]): T {
  if (answer?.kind !== kind) {
    throw new Error(`The daemon answered ${String(answer?.kind)} where ${kind} was expected`);
  }
  return answer as T;
}

/** Every Updates query starts with this, so one invalidation refreshes them. */
export const UPDATES_KEY = ["updates"] as const;

export async function fetchDigest(
  account: string | null,
  markSeen: boolean,
): Promise<UpdatesDigest> {
  const query = new URLSearchParams();
  if (account) query.set("account", account);
  if (markSeen) query.set("mark_seen", "true");
  const text = query.toString();
  const answer = await apiFetch<{ kind?: unknown }>(
    `/api/v1/mail/updates${text ? `?${text}` : ""}`,
  );
  return expectKind<Digest>(answer, "UpdatesDigest").digest;
}

/**
 * The digest for the account scope. The first fetch of each visit records
 * that Updates was opened, so "expired since you last looked" starts again
 * and the mute question counts as asked.
 */
export function useDigest() {
  const account = useUiPrefs((s) => s.accountScope);
  const scope = account ?? "all";
  const seen = useRef(new Set<string>());
  return useQuery({
    queryKey: [...UPDATES_KEY, "digest", scope],
    queryFn: () => {
      const markSeen = !seen.current.has(scope);
      seen.current.add(scope);
      return fetchDigest(account, markSeen);
    },
    staleTime: 15_000,
    refetchInterval: 60_000,
  });
}

export interface LetGoInput {
  account: string | null;
  /** The cut the digest showed (RFC 3339). */
  cut?: string;
  /** One source only. */
  sourceKey?: string;
  /** From the digest or the dry run: the run refuses if the cut changed. */
  selectionToken?: string;
  dryRun: boolean;
}

export async function letGoRequest(input: LetGoInput): Promise<UpdatesLetGo> {
  const answer = await apiFetch<{ kind?: unknown }>("/api/v1/mail/updates/let-go", {
    method: "POST",
    body: {
      account_id: input.account ?? undefined,
      cut: input.cut,
      source_key: input.sourceKey,
      selection_token: input.selectionToken,
      dry_run: input.dryRun,
    },
  });
  return expectKind<LetGo>(answer, "UpdatesLetGo").result;
}

export async function setSourceRequest(input: {
  accountId: string;
  source: string;
  setting: UpdateSetting;
  dryRun?: boolean;
}): Promise<UpdateSourceChange> {
  const answer = await apiFetch<{ kind?: unknown }>("/api/v1/mail/updates/sources", {
    method: "POST",
    body: {
      account_id: input.accountId,
      source: input.source,
      setting: input.setting,
      dry_run: input.dryRun ?? false,
    },
  });
  return expectKind<Source>(answer, "UpdateSource").change;
}
