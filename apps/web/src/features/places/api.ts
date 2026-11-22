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

export const PLACE_SLUG: Record<Place, "reading" | "paper-trail"> = {
  reading: "reading",
  paper_trail: "paper-trail",
};

export const PLACE_PAGE_SIZE = 50;

export interface PlacePageRequest {
  account?: string | null;
  sender?: string;
  offset?: number;
  messagesPerBundle: number;
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
  accountId?: string | null;
  senderEmail?: string;
}

export function previewSweep(scope: SweepScope): Promise<PlaceSwept> {
  return postSweep(scope, { dry_run: true });
}

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

export function messageKindQuery(messageId: string) {
  return {
    queryKey: messageKindKey(messageId),
    queryFn: () => fetchMessageKind(messageId),
    staleTime: 60_000,
    retry: false,
  };
}
