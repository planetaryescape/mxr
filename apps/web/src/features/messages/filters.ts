/*
 * What Messages keeps: one turn choice and two toggles (unread, past your
 * pace). The daemon applies it; this file only maps it to and from the URL
 * and the bridge query, so every place that reads or writes a filter agrees.
 */

import type { MessagesTurn } from "./api";

export interface MessagesFilter {
  /** `null` keeps every turn. */
  turn: MessagesTurn | null;
  unread: boolean;
  pastPace: boolean;
}

export const NO_FILTER: MessagesFilter = { turn: null, unread: false, pastPace: false };

/** The search params a filter lives in. Off values are absent, not false. */
export interface FilterSearch {
  turn?: MessagesTurn;
  unread?: boolean;
  pace?: boolean;
}

export function filterOf(search: FilterSearch): MessagesFilter {
  return {
    turn: search.turn ?? null,
    unread: search.unread === true,
    pastPace: search.pace === true,
  };
}

export function searchOf(filter: MessagesFilter): FilterSearch {
  return {
    ...(filter.turn ? { turn: filter.turn } : {}),
    ...(filter.unread ? { unread: true } : {}),
    ...(filter.pastPace ? { pace: true } : {}),
  };
}

/** A turn, a toggle or both: the list is narrower than the page's default. */
export function isFiltered(filter: MessagesFilter): boolean {
  return filter.turn !== null || filter.unread || filter.pastPace;
}
