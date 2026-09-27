/*
 * Reading and Paper trail over the bridge. Shapes come from the daemon's
 * OpenAPI schema; every rule (kinds, reasons, the sweep's selection) lives
 * in the daemon, so the web only asks and shows.
 */

import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

type Schemas = components["schemas"];

export type Place = Schemas["MailPlaceData"];
export type SenderKind = Schemas["SenderKindData"];
export type MailKind = Schemas["MailKindData"];
export type PlaceBundle = Schemas["PlaceBundleData"];
export type PlaceMessage = Schemas["PlaceMessageData"];
export type SweepPreview = Schemas["SweepPreviewData"];
export type PlaceResponse = Extract<Schemas["ResponseData"], { kind: "Place" }>;
export type PlaceSwept = Extract<Schemas["ResponseData"], { kind: "PlaceSwept" }>;
export type MessageKindResponse = Extract<Schemas["ResponseData"], { kind: "MessageKind" }>;
export type SenderKindSet = Extract<Schemas["ResponseData"], { kind: "SenderKindSet" }>;
export type MessagesPinned = Extract<Schemas["ResponseData"], { kind: "MessagesPinned" }>;

/** URL segment for each place, as the bridge and the app routes spell it. */
export const PLACE_SLUG: Record<Place, "reading" | "paper-trail"> = {
  reading: "reading",
  paper_trail: "paper-trail",
};

/** Senders per page of a place; "Load more" fetches the next page. */
export const PLACE_PAGE_SIZE = 50;

export interface PlacePageRequest {
  /** Omitted: every account (or the one `sender` belongs to). */
  account?: string | null;
  sender?: string;
  offset?: number;
  messagesPerBundle: number;
  /** Skip this many of each bundle's messages (pinned first, then newest). */
  messageOffset?: number;
}

export function fetchPlace(place: Place, request: PlacePageRequest): Promise<PlaceResponse> {
  const query = new URLSearchParams({
    limit: String(PLACE_PAGE_SIZE),
    offset: String(request.offset ?? 0),
    messages_per_bundle: String(request.messagesPerBundle),
  });
  if (request.account) query.set("account", request.account);
  if (request.sender) query.set("sender", request.sender);
  if (request.messageOffset) query.set("message_offset", String(request.messageOffset));
  return apiFetch<PlaceResponse>(`/api/v1/mail/places/${PLACE_SLUG[place]}?${query.toString()}`);
}

export interface SweepScope {
  place: Place;
  /** Omitted: every account (the app's scope decides). */
  accountId?: string | null;
  /** Omitted: the whole place. */
  senderEmail?: string;
}

/** The dry run: what a sweep would archive, and the token to commit it. */
export function previewSweep(scope: SweepScope): Promise<PlaceSwept> {
  return postSweep(scope, { dry_run: true });
}

/**
 * The sweep itself: archives exactly what the preview behind `previewToken`
 * listed that is still here and unpinned. The daemon refuses an expired or
 * used token, or one from another scope.
 */
export function commitSweep(scope: SweepScope, previewToken: string): Promise<PlaceSwept> {
  return postSweep(scope, { dry_run: false, preview_token: previewToken });
}

function postSweep(
  scope: SweepScope,
  body: { dry_run: boolean; preview_token?: string },
): Promise<PlaceSwept> {
  return apiFetch<PlaceSwept>(`/api/v1/mail/places/${PLACE_SLUG[scope.place]}/sweep`, {
    method: "POST",
    body: {
      account_id: scope.accountId ?? undefined,
      sender_email: scope.senderEmail,
      ...body,
    },
  });
}

export function pinMessages(messageIds: string[], pinned: boolean): Promise<MessagesPinned> {
  return apiFetch<MessagesPinned>("/api/v1/mail/messages/pin", {
    method: "POST",
    body: { message_ids: messageIds, pinned },
  });
}

export function setSenderKind(input: {
  accountId: string;
  senderEmail: string;
  kind: SenderKind | null;
}): Promise<SenderKindSet> {
  return apiFetch<SenderKindSet>("/api/v1/mail/senders/kind", {
    method: "POST",
    body: { account_id: input.accountId, sender_email: input.senderEmail, kind: input.kind },
  });
}

function messageKindKey(messageId: string) {
  return ["message-kind", messageId] as const;
}

function fetchMessageKind(messageId: string): Promise<MessageKindResponse> {
  return apiFetch<MessageKindResponse>(
    `/api/v1/mail/messages/${encodeURIComponent(messageId)}/kind`,
  );
}

/** Shared by the reader's "why here" line and its Move sender command. */
export function messageKindQuery(messageId: string) {
  return {
    queryKey: messageKindKey(messageId),
    queryFn: () => fetchMessageKind(messageId),
    staleTime: 60_000,
    retry: false,
  };
}
