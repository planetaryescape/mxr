/*
 * Reading's verbs: later, let go, fetch the linked article, highlight and
 * the sender's own layout. Each refreshes the edition when it lands; let
 * go is the modes' done, so its toast names where the item went and `u`
 * undoes it through the daemon.
 */

import { toast } from "sonner";

import { claimUndo, offerUndo } from "@/features/mail-actions/mailUndo";
import { markModeDone } from "@/features/modes/modeDone";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  fetchArticle,
  READING_KEY,
  saveHighlight,
  setLater,
  setSource,
  type ReadingFetch,
} from "./api";
import { domainLabel, highlightPayload } from "./readingView";

export async function refreshReading(): Promise<void> {
  await getActiveQueryClient()
    ?.invalidateQueries({ queryKey: READING_KEY })
    .catch(() => undefined);
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** What the reader and toasts say about a fetch. */
export function fetchedLine(fetch: ReadingFetch): string {
  if (fetch.error) return fetch.error;
  if (fetch.cached) return "Saved earlier; nothing was contacted.";
  const hosts = fetch.article?.contacted ?? [];
  return hosts.length > 0 ? `Fetched from ${hosts.join(", ")}.` : "Saved from the demo's copy.";
}

/**
 * `L`: fetch the linked article. Contacts the link's site, so the toast
 * names it before anything is sent.
 */
export async function fetchLinked(
  itemKey: string,
  domain: string | null | undefined,
  tracked?: boolean,
): Promise<ReadingFetch | null> {
  if (refuseWhileDaemonDown("fetch the article")) return null;
  const site = domainLabel(domain, tracked) || "the article's site";
  const id = `reading-fetch-${itemKey}`;
  toast.loading(`Fetching from ${site}…`, { id });
  try {
    const fetched = await fetchArticle(itemKey);
    if (fetched.error) {
      toast.error("Couldn't fetch the article", { id, description: fetched.error });
    } else {
      toast.success(fetched.cached ? "Article saved earlier" : "Article saved", {
        id,
        description: fetchedLine(fetched),
      });
    }
    await refreshReading();
    return fetched;
  } catch (error) {
    toast.error("Couldn't fetch the article", { id, description: errorText(error) });
    return null;
  }
}

/**
 * `b`: put an item on Later, then save its article so it reads offline. A
 * link's article is fetched at once, naming the site in the toast.
 */
export async function putOnLater(
  itemKey: string,
  article?: { domain?: string | null; tracked?: boolean; cached?: boolean },
): Promise<boolean> {
  if (refuseWhileDaemonDown("save it for later")) return false;
  const claim = claimUndo();
  try {
    const answer = await setLater([itemKey], true);
    const failed = answer.items.find((item) => item.error);
    if (failed) {
      claim.settle(null);
      toast.error("Couldn't save it for later", { description: failed.error ?? undefined });
      return false;
    }
    claim.settle(
      offerUndo(
        "reading-later",
        answer.copy,
        `reading-later-${itemKey}`,
        async () => {
          if (refuseWhileDaemonDown("undo")) return false;
          try {
            await setLater([itemKey], false);
            toast.success("Undone");
            return true;
          } catch (error) {
            toast.error("Undo failed", { description: errorText(error) });
            return false;
          } finally {
            await refreshReading();
          }
        },
        claim.run,
      ),
    );
    await refreshReading();
    if (article?.domain && !article.cached)
      await fetchLinked(itemKey, article.domain, article.tracked);
    return true;
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't save it for later", { description: errorText(error) });
    return false;
  }
}

/** Take items off Later, or answer "Still want it?" with keep (`later`). */
export async function answerLater(itemKey: string, keep: boolean): Promise<void> {
  if (refuseWhileDaemonDown(keep ? "keep it" : "take it off Later")) return;
  try {
    const answer = await setLater([itemKey], keep);
    toast.success(keep ? "Kept on Later." : answer.copy);
    await refreshReading();
  } catch (error) {
    toast.error("Couldn't change Later", { description: errorText(error) });
  }
}

/** `e`: let go of the issue in Reading (its links on Later stay). */
export async function letGo(threadIds: readonly string[]): Promise<boolean> {
  const done = await markModeDone("reading", threadIds, { onUndone: () => void refreshReading() });
  await refreshReading();
  return done;
}

/** `h`: save the selected passage. */
export async function highlightSelection(
  selection: string,
  itemKey: string,
  view: "issue" | "article",
): Promise<boolean> {
  const payload = highlightPayload(selection, itemKey, view);
  if (!payload) {
    toast.info("Select some text first, then press h");
    return false;
  }
  try {
    await saveHighlight(payload);
    toast.success("Highlight saved", {
      description: "Search finds it, and mxr reading export --markdown lists it.",
    });
    await getActiveQueryClient()?.invalidateQueries({
      queryKey: [...READING_KEY, "item", itemKey],
    });
    return true;
  } catch (error) {
    toast.error("Couldn't save the highlight", { description: errorText(error) });
    return false;
  }
}

/** `R`: remember the sender's own layout for this source. */
export async function rememberLayout(
  accountId: string,
  senderEmail: string,
  original: boolean,
): Promise<void> {
  try {
    await setSource({ accountId, senderEmail, originalLayout: original });
  } catch (error) {
    toast.error("Couldn't remember the layout for this source", {
      description: errorText(error),
    });
  }
}

/** "Not interested in these offers": never offer to unsubscribe from it again. */
export async function dismissUnsubscribeOffer(accountId: string, senderEmail: string) {
  try {
    await setSource({ accountId, senderEmail, dismissUnsubscribeOffer: true });
    await refreshReading();
  } catch (error) {
    toast.error("Couldn't change this source", { description: errorText(error) });
  }
}
