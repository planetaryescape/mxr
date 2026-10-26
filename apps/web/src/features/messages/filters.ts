/*
 * What Messages keeps: one turn choice and two toggles (unread, past your
 * pace). The daemon applies it; this file only maps it to and from the URL
 * and the bridge query, so every place that reads or writes a filter agrees.
 */

import type { MessagesTurn } from "./api";

export interface MessagesFilter {
  /** `null` selects Needs attention; `all` explicitly includes history. */
  turn: MessagesTurn | null;
  unread: boolean;
  pastPace: boolean;
}

export const NO_FILTER: MessagesFilter = { turn: null, unread: false, pastPace: false };

/** The order shared by the view picker and its cycle shortcut. */
export const MESSAGES_VIEWS = [
  ["attention", "Needs attention", null],
  ["all", "All conversations", "all"],
  ["mine", "Your turn", "mine"],
  ["theirs", "Waiting on them", "theirs"],
] as const satisfies readonly (readonly [string, string, MessagesTurn | null])[];

export function cycleMessagesFilter(filter: MessagesFilter): MessagesFilter {
  const current = MESSAGES_VIEWS.findIndex(([, , turn]) => turn === filter.turn);
  const next = MESSAGES_VIEWS[(current + 1) % MESSAGES_VIEWS.length]!;
  return { ...filter, turn: next[2] };
}

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
  const search: FilterSearch = {};
  if (filter.turn) search.turn = filter.turn;
  if (filter.unread) search.unread = true;
  if (filter.pastPace) search.pace = true;
  return search;
}

/** An explicit view or toggle differs from the attention-only default. */
export function isFiltered(filter: MessagesFilter): boolean {
  return filter.turn !== null || filter.unread || filter.pastPace;
}
