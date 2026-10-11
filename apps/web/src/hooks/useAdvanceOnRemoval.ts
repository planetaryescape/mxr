/*
 * Keep a detail pane off a row that has left its list, and bring it back
 * on undo. When the selected row disappears (done, archived elsewhere,
 * removed by sync), open its neighbour by `nextAfterRemoval`, or call
 * `onEmpty` when nothing near it is left. When a row the selection just
 * moved away from comes back while the selection is still where it went
 * (`u`, the toast's Undo), open it again. Verbs that move on by themselves
 * (the reader's archive, Messages' done) change the selection first; this
 * then only remembers the move for undo.
 */

import { useRouter } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";

import { nextAfterRemoval } from "@/lib/listAdvance";

/** A move counts as the row leaving when the row goes within this long. */
const LEAVE_WINDOW_MS = 2_000;
/** As long as an undo is offered (the result toast's duration). */
const UNDO_WINDOW_MS = 60_000;

export interface AdvanceOnRemoval {
  /** Row ids in display order. */
  ids: readonly string[];
  selectedId: string | null;
  /**
   * Whether a row still exists when it can sit outside `ids` (a folded
   * band). Defaults to membership in `ids`.
   */
  isPresent?: (id: string) => boolean;
  /** False while the list is loading or belongs to another scope. */
  ready?: boolean;
  /**
   * Open `to`, moving off `from`: a neighbour of a row that left, or a row
   * undo brought back (`from` is null when the pane had closed).
   */
  onAdvance: (to: string, from: string | null) => void;
  onEmpty: (removed: string) => void;
}

export type ExpectRemoval = (id: string, direction: 1 | -1) => (keepForRemoval?: boolean) => void;

interface Move {
  from: string;
  /** Where the selection went; null when the pane closed. */
  to: string | null;
  at: number;
}

export function useAdvanceOnRemoval({
  ids,
  selectedId,
  isPresent,
  ready = true,
  onAdvance,
  onEmpty,
}: AdvanceOnRemoval): ExpectRemoval {
  const previous = useRef<readonly string[]>([]);
  const previousSelected = useRef<string | null>(null);
  // The row already moved on from, so a render before the new selection
  // lands doesn't move twice.
  const handled = useRef<string | null>(null);
  // The latest selection change, and the one that followed a row leaving.
  const moved = useRef<Move | null>(null);
  const restore = useRef<Move | null>(null);
  const expected = useRef(
    new Map<
      string,
      { direction: 1 | -1; token: symbol; completed: boolean; expiresAt: number | null }
    >(),
  );
  const [expectedRevision, setExpectedRevision] = useState(0);
  // Rows that just left, with the list they left: a selection that lands
  // on one a moment later (a click racing the verb) still moves on.
  const gone = useRef(new Map<string, { list: readonly string[]; at: number }>());
  const latest = useRef({ isPresent, onAdvance, onEmpty });
  latest.current = { isPresent, onAdvance, onEmpty };
  const expectRemoval = useCallback<ExpectRemoval>((id, direction) => {
    const token = Symbol(id);
    expected.current.set(id, { direction, token, completed: false, expiresAt: null });
    return (keepForRemoval = true) => {
      const intent = expected.current.get(id);
      if (intent?.token !== token) return;
      if (!keepForRemoval) expected.current.delete(id);
      intent.completed = true;
      // A fallback can move first; keep its removal linked to Undo only while Undo is available.
      intent.expiresAt = Date.now() + UNDO_WINDOW_MS;
      setExpectedRevision((revision) => revision + 1);
    };
  }, []);

  useEffect(() => {
    if (!ready) return;
    const before = previous.current;
    previous.current = ids;
    const wasSelected = previousSelected.current;
    previousSelected.current = selectedId;
    const now = Date.now();
    const present = latest.current.isPresent ?? ((id: string) => ids.includes(id));
    for (const [id, intent] of expected.current)
      if (intent.expiresAt !== null && now > intent.expiresAt) expected.current.delete(id);
    for (const [id, left] of gone.current) {
      if (now - left.at > LEAVE_WINDOW_MS || present(id)) gone.current.delete(id);
    }
    for (const id of before) {
      if (!present(id) && !gone.current.has(id)) gone.current.set(id, { list: before, at: now });
    }

    const abandonedExpectedRemoval =
      wasSelected !== null &&
      wasSelected !== selectedId &&
      expected.current.get(wasSelected)?.completed === false &&
      present(wasSelected);
    if (abandonedExpectedRemoval) expected.current.delete(wasSelected);
    if (wasSelected && wasSelected !== selectedId) {
      moved.current = abandonedExpectedRemoval
        ? null
        : { from: wasSelected, to: selectedId, at: now };
    }
    // Moving anywhere else gives up the way back.
    const way = restore.current;
    if (way && selectedId !== way.to && selectedId !== way.from) restore.current = null;

    // A verb moved on first and its row has now gone: remember it for undo.
    const move = moved.current;
    if (
      move &&
      move.to === selectedId &&
      (now - move.at < LEAVE_WINDOW_MS || expected.current.has(move.from)) &&
      before.includes(move.from) &&
      !present(move.from)
    ) {
      restore.current = move;
      moved.current = null;
      expected.current.delete(move.from);
    }

    // Undo brought back the row we moved off: open it again.
    const back = restore.current;
    if (back && back.to === selectedId && present(back.from)) {
      restore.current = null;
      if (now - back.at < UNDO_WINDOW_MS) latest.current.onAdvance(back.from, back.to);
      return;
    }

    if (!selectedId) return;
    if (present(selectedId)) {
      handled.current = null;
      return;
    }
    // Never in this list (opened from a link, another band): not ours to move.
    const list = before.includes(selectedId) ? before : gone.current.get(selectedId)?.list;
    if (handled.current === selectedId || !list) return;
    handled.current = selectedId;
    const intent = expected.current.get(selectedId);
    if (intent) expected.current.delete(selectedId);
    const ordered = intent?.direction === -1 ? list.toReversed() : list;
    const next = nextAfterRemoval(ordered, selectedId, present);
    restore.current = { from: selectedId, to: next, at: now };
    if (next) latest.current.onAdvance(next, selectedId);
    else latest.current.onEmpty(selectedId);
  }, [expectedRevision, ids, ready, selectedId]);

  return expectRemoval;
}

/**
 * Whether the URL still ends at this conversation, read from history so a
 * navigation already on its way counts: the reader's archive-and-advance
 * moves first, and a list's follow-up must not override where it went.
 */
export function useRouteStillShows(): (threadId: string) => boolean {
  const router = useRouter();
  return useCallback(
    (threadId: string) =>
      router.history.location.pathname.endsWith(`/${encodeURIComponent(threadId)}`),
    [router],
  );
}
