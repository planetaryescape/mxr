import { usePromise } from "@raycast/utils";
import { webBase } from "../bridge/client";

export const web = {
  mode: (mode: "now" | "messages" | "todo" | "updates" | "reading" | "archive") => `/${mode}`,
  thread: (mode: "messages" | "todo" | "updates" | "reading" | "archive", threadId: string) =>
    `/${mode}/${encodeURIComponent(threadId)}`,
  sourceMessage: (source: { from_me: boolean; thread_id: string; message_id: string }) =>
    `/m/${source.from_me ? "sent" : "archive"}/${encodeURIComponent(source.thread_id)}?${new URLSearchParams({ message: source.message_id })}`,
  person: (personId: string, topic?: string) =>
    `/messages?${new URLSearchParams({ person: personId, ...(topic ? { topic } : {}) })}`,
  todo: (todoId: string) => `/todo?${new URLSearchParams({ row: todoId })}`,
  readingItem: (itemKey: string) => `/reading/item/${encodeURIComponent(itemKey)}`,
  search: (threadId: string) => `/search/${encodeURIComponent(threadId)}`,
  inbox: (threadId?: string) =>
    threadId ? `/m/inbox/${encodeURIComponent(threadId)}` : "/m/inbox",
  draft: (draftId: string) => `/compose/${encodeURIComponent(draftId)}`,
};

export function useWebBase(): string | undefined {
  return usePromise(webBase).data;
}
