import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type EntityExplanation = components["schemas"]["EntityExplanationData"];
export type ArchiveAnswer = components["schemas"]["ArchiveAnswerData"];

export function fetchWhois(entity: string, accountId?: string) {
  const query = new URLSearchParams({ query: entity });
  if (accountId) query.set("account", accountId);
  return apiFetch<{ kind: "EntityExplanation"; entity: EntityExplanation }>(
    `/api/v1/mail/whois?${query.toString()}`,
  );
}

export function askArchive(question: string, limit?: number) {
  return apiFetch<{ kind: "ArchiveAnswer"; answer: ArchiveAnswer }>("/api/v1/mail/archive-ask", {
    method: "POST",
    body: { question, limit },
  });
}
