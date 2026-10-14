/*
 * Reading's verbs: later, let go, fetch the linked article, highlight and
 * the sender's own layout. Each refreshes the edition when it lands; let
 * go is the modes' done, so its toast names where the item went and `u`
 * undoes it through the daemon.
 */

import { toast } from "sonner";

import { claimUndo, offerUndo, performUndo } from "@/features/mail-actions/mailUndo";
import { markModeDone } from "@/features/modes/modeDone";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  fetchArticle,
  READING_KEY,
  removeHighlightRequest,
  updateHighlightNote,
  saveHighlight,
  sendToMarginalia,
  setLater,
  setSource,
  syncMarginaliaHighlights,
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
  note?: string,
): Promise<boolean> {
  const payload = highlightPayload(selection, itemKey, view);
  if (!payload) {
    toast.info("Select some text first, then press h");
    return false;
  }
  try {
    await saveHighlight({ ...payload, note: note?.trim() || undefined });
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

/**
 * Changes a highlight (note or removal) and offers the daemon's undo, which
 * puts the old note or the whole highlight back.
 */
async function changeHighlight(
  itemKey: string,
  message: string,
  change: () => Promise<{ mutation_id?: string | null; undo_unavailable?: boolean }>,
): Promise<boolean> {
  const refresh = () =>
    getActiveQueryClient()?.invalidateQueries({ queryKey: [...READING_KEY, "item", itemKey] });
  const claim = claimUndo();
  try {
    const result = await change();
    const mutationId = result.mutation_id ?? null;
    const reverse = mutationId
      ? async () => {
          const undone = await performUndo(mutationId);
          if (undone) await refresh();
          return undone;
        }
      : null;
    claim.settle(
      offerUndo(
        "mode-done",
        message,
        `highlight-${mutationId ?? itemKey}`,
        reverse,
        claim.run,
        mutationId ?? undefined,
      ),
    );
    if (result.undo_unavailable) toast.warning("Done, but its undo couldn't be saved");
    await refresh();
    return true;
  } catch (error) {
    claim.settle(null);
    toast.error("Couldn't change the highlight", { description: errorText(error) });
    return false;
  }
}

/** Replaces a highlight's note; a blank note clears it. */
export function editHighlightNote(itemKey: string, id: string, note: string) {
  return changeHighlight(itemKey, note.trim() ? "Note saved" : "Note cleared", () =>
    updateHighlightNote(id, note),
  );
}

export function removeHighlight(itemKey: string, id: string) {
  return changeHighlight(itemKey, "Highlight removed", () => removeHighlightRequest(id));
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

/**
 * Only web addresses are opened. The daemon already refuses read URLs off
 * the connected site; this keeps a `javascript:` URL from ever reaching
 * an about:blank tab, which shares mxr's origin.
 */
export function isWebUrl(url: string): boolean {
  try {
    const { protocol } = new URL(url);
    return protocol === "https:" || protocol === "http:";
  } catch {
    return false;
  }
}

/** Opens a page in a new tab that can't reach back into mxr. */
function openTab(url: string): void {
  if (!isWebUrl(url)) {
    toast.error("Marginalia gave an address mxr won't open");
    return;
  }
  const tab = window.open(url, "_blank", "noopener,noreferrer");
  if (!tab) toast.message("Your browser blocked the new tab", { description: url });
}

/**
 * `M`: open the item in Marginalia, sending it first if it hasn't been.
 * Sending gives the email's text to Marginalia, so it only happens on
 * this key or button. The tab opens on the keypress, before the send
 * finishes, so the browser doesn't block it.
 */
export async function openInMarginalia(
  itemKey: string,
  sentUrl: string | null | undefined,
  options: { connected: boolean; onConnect: () => void },
): Promise<boolean> {
  if (sentUrl) {
    openTab(sentUrl);
    return true;
  }
  if (!options.connected) {
    toast.message("Marginalia isn't connected", {
      description: "Connect it once in Settings, with a token from its Account page.",
      action: { label: "Connect", onClick: options.onConnect },
    });
    return false;
  }
  if (refuseWhileDaemonDown("send it to Marginalia")) return false;
  const tab = window.open("about:blank", "_blank");
  if (tab) tab.opener = null;
  const id = `reading-marginalia-${itemKey}`;
  toast.loading("Sending to Marginalia…", { id });
  try {
    const sent = await sendToMarginalia(itemKey);
    const url = sent.marginalia?.read_url;
    if (url && tab && isWebUrl(url)) tab.location.href = url;
    else if (url && !tab) openTab(url);
    else {
      tab?.close();
      if (url) toast.error("Marginalia gave an address mxr won't open");
    }
    toast.success(sent.copy, {
      id,
      description: "Highlights you make there come back to this item.",
    });
    await refreshReading();
    return true;
  } catch (error) {
    tab?.close();
    toast.error("Couldn't send it to Marginalia", { id, description: errorText(error) });
    return false;
  }
}

/** Reads back the item's Marginalia highlights, quietly, when it opens. */
export async function refreshMarginaliaHighlights(itemKey: string): Promise<void> {
  try {
    const sync = await syncMarginaliaHighlights(itemKey);
    if (sync.added + sync.removed + sync.forgotten > 0) await refreshReading();
  } catch {
    // Offline or Marginalia unavailable: the saved highlights still show.
  }
}
