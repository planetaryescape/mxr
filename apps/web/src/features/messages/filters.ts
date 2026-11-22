import type { MessagesTurn } from "./api";

export interface MessagesFilter {
  turn: MessagesTurn | null;
  unread: boolean;
  pastPace: boolean;
}

export const NO_FILTER: MessagesFilter = { turn: null, unread: false, pastPace: false };

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

export function isFiltered(filter: MessagesFilter): boolean {
  return filter.turn !== null || filter.unread || filter.pastPace;
}
