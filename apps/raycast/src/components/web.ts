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
  /** A stored draft in the web composer. */
  draft: (draftId: string) => `/compose/${encodeURIComponent(draftId)}`,
};

/** The web app's address; links render once it is known. */
export function useWebBase(): string | undefined {
  return usePromise(webBase).data;
}
