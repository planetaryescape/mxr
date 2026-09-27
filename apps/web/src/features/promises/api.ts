/*
 * Promises on send: what an outgoing compose session promises (asked while
 * its undo window runs), and keeping one as a dated commitment once the
 * message has sent. Types come from the generated protocol schema.
 */

import { apiFetch } from "@/api/client";
import type { components, operations } from "@/api/generated";
import { browserTimeZone } from "@/features/time/api";

type Schemas = components["schemas"];
export type PromiseDetection = Schemas["PromiseDetectionData"];
export type DetectedPromise = Schemas["DetectedPromiseData"];
export type Commitment = Schemas["CommitmentData"];

type DetectBody =
  operations["compose_session_promises"]["requestBody"]["content"]["application/json"];
type RecordBody =
  operations["mail_commitments_record"]["requestBody"]["content"]["application/json"];
type DetectResponse = Extract<Schemas["ResponseData"], { kind: "Promises" }>;
type RecordResponse = Extract<Schemas["ResponseData"], { kind: "RecordedPromise" }>;

/** Due words resolve in the browser's zone: "by Friday" is where you are. */
export async function detectComposePromises(
  draftPath: string,
  accountId: string,
): Promise<PromiseDetection> {
  const body: DetectBody = {
    draft_path: draftPath,
    account_id: accountId,
    time_zone: browserTimeZone(),
  };
  const response = await apiFetch<DetectResponse>("/api/v1/mail/compose/session/promises", {
    method: "POST",
    body,
  });
  return response.detection;
}

/** Keep a promise from a sent message, due at the instant the user chose. */
export async function recordPromise(
  messageId: string,
  what: string,
  dueAt: Date,
): Promise<Commitment> {
  const body: RecordBody = { message_id: messageId, what, due_at: dueAt.toISOString() };
  const response = await apiFetch<RecordResponse>("/api/v1/mail/commitments", {
    method: "POST",
    body,
  });
  return response.commitment;
}
