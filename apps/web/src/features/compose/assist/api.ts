/*
 * The writing-help request. The bridge route is named once here, so a
 * rename is a one-line change.
 */

import { apiFetch } from "@/api/client";
import type { ComposeAssistRequest, ComposeAssistResponse } from "./types";

export const COMPOSE_ASSIST_PATH = "/api/v1/mail/compose/assist";

export function requestComposeAssist(
  request: ComposeAssistRequest,
): Promise<ComposeAssistResponse> {
  return apiFetch<ComposeAssistResponse>(COMPOSE_ASSIST_PATH, {
    method: "POST",
    body: request,
  });
}
