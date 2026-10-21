import { usePromise } from "@raycast/utils";
import { webBase } from "../bridge/client";

/** Pages of the web app, which the daemon serves on its bridge port. The
 * page fetches its own token from the daemon, so links carry none. */
export const web = {
  mode: (mode: "now" | "messages" | "todo" | "updates" | "reading" | "archive") => `/${mode}`,
  thread: (mode: "messages" | "todo" | "updates" | "reading" | "archive", threadId: string) =>
    `/${mode}/${encodeURIComponent(threadId)}`,
  person: (personId: string, topic?: string) =>
    `/messages?${new URLSearchParams({ person: personId, ...(topic ? { topic } : {}) })}`,
  todo: (todoId: string) => `/todo?${new URLSearchParams({ row: todoId })}`,
  readingItem: (itemKey: string) => `/reading/item/${encodeURIComponent(itemKey)}`,
  reply: (messageId: string, mode: "single" | "all") =>
    `/compose/new?${new URLSearchParams({ reply: messageId, mode })}`,
};

/** The web app's address; links render once it is known. */
export function useWebBase(): string | undefined {
  return usePromise(webBase).data;
}
