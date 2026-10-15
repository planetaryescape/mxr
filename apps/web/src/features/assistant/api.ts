import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

type Schemas = components["schemas"];
export type AssistantAvailability = Schemas["AssistantAvailabilityData"];
export type AssistantConversation = Schemas["AssistantConversationData"];
export type AssistantSummary = Schemas["AssistantConversationSummaryData"];
export type AssistantItem = Schemas["AssistantItemData"];
export type AssistantPending = Schemas["AssistantPendingData"];
export type AssistantQuestion = Schemas["AssistantQuestionData"];
export type AssistantDecision = Schemas["AssistantDecisionData"];
export type AssistantAnswer = Schemas["AssistantAnswerData"];
export type AssistantProgress = Schemas["AssistantProgressData"];
export type AssistantViewContext = Schemas["AssistantViewContextData"];
export type AssistantCallState = Schemas["AssistantCallStateData"];

const BASE = "/api/v1/mail/assistant";

export const assistantKeys = {
  all: ["assistant"] as const,
  availability: ["assistant", "availability"] as const,
  list: ["assistant", "conversations"] as const,
  conversation: (id: string) => ["assistant", "conversation", id] as const,
};

export async function fetchAvailability(): Promise<AssistantAvailability> {
  const response = await apiFetch<{ availability: AssistantAvailability }>(`${BASE}/availability`);
  return response.availability;
}

export async function fetchConversations(): Promise<AssistantSummary[]> {
  const response = await apiFetch<{ conversations: AssistantSummary[] }>(
    `${BASE}/conversations?limit=50`,
  );
  return response.conversations;
}

export async function fetchConversation(id: string): Promise<AssistantConversation> {
  const response = await apiFetch<{ conversation: AssistantConversation }>(
    `${BASE}/conversations/${encodeURIComponent(id)}`,
  );
  return response.conversation;
}

export async function sendMessage(input: {
  conversationId: string | null;
  message: string;
  context: AssistantViewContext;
}): Promise<AssistantConversation> {
  const response = await apiFetch<{ conversation: AssistantConversation }>(
    `${BASE}/conversations`,
    {
      method: "POST",
      body: {
        conversation_id: input.conversationId ?? undefined,
        message: input.message,
        context: input.context,
      },
    },
  );
  return response.conversation;
}

export async function decide(
  conversationId: string,
  decisions: AssistantDecision[],
): Promise<AssistantConversation> {
  const response = await apiFetch<{ conversation: AssistantConversation }>(
    `${BASE}/conversations/${encodeURIComponent(conversationId)}/decide`,
    { method: "POST", body: { decisions } },
  );
  return response.conversation;
}

export async function cancelTurn(conversationId: string): Promise<AssistantConversation> {
  const response = await apiFetch<{ conversation: AssistantConversation }>(
    `${BASE}/conversations/${encodeURIComponent(conversationId)}/cancel`,
    { method: "POST" },
  );
  return response.conversation;
}

export async function deleteConversation(conversationId: string): Promise<void> {
  await apiFetch(`${BASE}/conversations/${encodeURIComponent(conversationId)}/delete`, {
    method: "POST",
    body: { dry_run: false },
  });
}
