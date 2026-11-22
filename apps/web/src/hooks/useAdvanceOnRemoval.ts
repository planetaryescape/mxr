import { useRouter } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";

import { nextAfterRemoval } from "@/lib/listAdvance";

const LEAVE_WINDOW_MS = 2_000;
const UNDO_WINDOW_MS = 60_000;

export interface AdvanceOnRemoval {
  ids: readonly string[];
  selectedId: string | null;
  isPresent?: (id: string) => boolean;
  presenceSource?: unknown;
  ready?: boolean;
  onAdvance: (to: string, from: string | null) => void;
  onEmpty: (removed: string) => void;
}

export type ExpectRemoval = (id: string, direction: 1 | -1) => (keepForRemoval?: boolean) => void;

interface Move {
  from: string;
  to: string | null;
  at: number;
}

export function useAdvanceOnRemoval({
  ids,
  selectedId,
  isPresent,
  presenceSource,
  ready = true,
  onAdvance,
  onEmpty,
}: AdvanceOnRemoval): ExpectRemoval {
  const previous = useRef<readonly string[]>([]);
  const previousSelected = useRef<string | null>(null);
  const handled = useRef<string | null>(null);
  const moved = useRef<Move | null>(null);
  const restore = useRef<Move | null>(null);
  const expected = useRef(
    new Map<
      string,
      { direction: 1 | -1; token: symbol; completed: boolean; expiresAt: number | null }
    >(),
  );
  const [expectedRevision, setExpectedRevision] = useState(0);
  const gone = useRef(new Map<string, { list: readonly string[]; at: number }>());
  const scanned = useRef<{ list: readonly string[]; source: unknown } | null>(null);
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
    const source = latest.current.isPresent ? presenceSource : ids;
    const last = scanned.current;
    const unchanged =
      last !== null && last.list === before && source !== undefined && last.source === source;
    if (!unchanged) {
      for (const id of before) {
        if (!present(id) && !gone.current.has(id)) gone.current.set(id, { list: before, at: now });
      }
      scanned.current = { list: before, source };
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
    const way = restore.current;
    if (way && selectedId !== way.to && selectedId !== way.from) restore.current = null;

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
  }, [expectedRevision, ids, presenceSource, ready, selectedId]);

  return expectRemoval;
}

export function useRouteStillShows(): (threadId: string) => boolean {
  const router = useRouter();
  return useCallback(
    (threadId: string) =>
      router.history.location.pathname.endsWith(`/${encodeURIComponent(threadId)}`),
    [router],
  );
}
