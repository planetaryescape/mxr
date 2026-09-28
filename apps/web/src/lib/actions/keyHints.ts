/*
 * The app teaches its keys, quietly. Every pointer route to an action that
 * has a key (toolbar buttons, row buttons, bulk bar, focus buttons) reports
 * here. After the third pointer use of the same action, one hint names its
 * key, once per action ever, never while the keyboard is in use, and never
 * more than one hint every few minutes. Counts live in this browser only;
 * without storage there are no hints, which is fine.
 */

import { toast } from "sonner";

import { formatChord } from "@/lib/keys/chord";
import { runCommand } from "@/lib/keys/controllers";

import { chordsOf, getRegistry, scopesOf } from "./registry";
import type { Action, ActionScope } from "./types";

export const POINTER_USES_BEFORE_HINT = 3;
/** At most one hint in this window, whatever it is for. */
export const HINT_GAP_MS = 5 * 60_000;
/** A key pressed this recently means the person is on the keyboard. */
export const KEYBOARD_QUIET_MS = 10_000;

const STORAGE_KEY = "mxr.keyHints";

interface HintMemory {
  counts: Record<string, number>;
  shown: string[];
  lastShownAt: number;
}

export interface HintStore {
  load: () => HintMemory | null;
  save: (memory: HintMemory) => void;
}

export interface KeyHint {
  actionId: string;
  keys: string;
  text: string;
}

const EMPTY: HintMemory = { counts: {}, shown: [], lastShownAt: 0 };

/**
 * The counting rule, free of the DOM so it can be tested with a clock. Call
 * `pointer` for each pointer use; it returns the hint to show, if any.
 */
export function createHintTracker(store: HintStore, now: () => number) {
  let lastKeyAt = Number.NEGATIVE_INFINITY;
  return {
    keyboard(): void {
      lastKeyAt = now();
    },
    pointer(action: Action, name?: string): KeyHint | null {
      const keys = chordsOf(action)[0];
      if (!keys || action.paletteOnly) return null;
      const memory = store.load();
      if (!memory || memory.shown.includes(action.id)) return null;
      const count = (memory.counts[action.id] ?? 0) + 1;
      memory.counts[action.id] = count;
      const time = now();
      const due =
        count >= POINTER_USES_BEFORE_HINT &&
        time - memory.lastShownAt >= HINT_GAP_MS &&
        time - lastKeyAt >= KEYBOARD_QUIET_MS;
      if (due) {
        memory.shown.push(action.id);
        memory.lastShownAt = time;
        delete memory.counts[action.id];
      }
      store.save(memory);
      return due ? hintFor(action, keys, name) : null;
    },
  };
}

/** "Tip: press E for Archive." `name` overrides the action's own words. */
export function hintFor(action: Action, chord: string, name?: string): KeyHint {
  const keys = formatChord(chord);
  const what = (name ?? action.shortLabel ?? action.label).replace(/…$/, "");
  return { actionId: action.id, keys, text: `Tip: press ${keys} for ${what}.` };
}

/** Stored memory is the viewer's to edit; anything malformed starts over. */
export function parseHintMemory(value: unknown): HintMemory {
  if (typeof value !== "object" || value === null) return structuredClone(EMPTY);
  const { counts, shown, lastShownAt } = value as Record<string, unknown>;
  const cleanCounts: Record<string, number> = {};
  if (typeof counts === "object" && counts !== null) {
    for (const [id, count] of Object.entries(counts)) {
      if (typeof count === "number" && Number.isFinite(count)) cleanCounts[id] = count;
    }
  }
  return {
    counts: cleanCounts,
    shown: Array.isArray(shown) ? shown.filter((id): id is string => typeof id === "string") : [],
    lastShownAt: typeof lastShownAt === "number" && Number.isFinite(lastShownAt) ? lastShownAt : 0,
  };
}

const localStore: HintStore = {
  load: () => {
    try {
      const raw = window.localStorage.getItem(STORAGE_KEY);
      if (raw === null) return structuredClone(EMPTY);
      return parseHintMemory(JSON.parse(raw));
    } catch {
      return null;
    }
  },
  save: (memory) => {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(memory));
    } catch {
      // No storage, no hints.
    }
  },
};

const tracker = createHintTracker(localStore, () => Date.now());

/**
 * Note a pointer use of the action with this id; may show its hint. `name`
 * is what the button said, for actions whose own label is generic.
 */
export function notePointerUse(actionId: string, name?: string): void {
  const action = getRegistry().get(actionId);
  if (!action) return;
  const hint = tracker.pointer(action, name);
  if (hint) {
    // Its own spot, bottom centre: never stacked over the action's undo.
    toast(hint.text, {
      id: "key-hint",
      duration: 8000,
      className: "key-hint",
      position: "bottom-center",
    });
  }
}

/** Note a pointer use of whichever keyed action runs `command` in `scope`. */
export function notePointerCommand(scope: ActionScope, command: string): void {
  const action = getRegistry()
    .all()
    .find(
      (candidate) =>
        candidate.command === command &&
        scopesOf(candidate).includes(scope) &&
        chordsOf(candidate).length > 0,
    );
  if (action) notePointerUse(action.id);
}

/**
 * Run a scope command from a button, and count it toward that action's
 * key hint. The pointer twin of the key dispatcher's `invokeAction`.
 */
export function runPointerCommand(scope: ActionScope, command: string): boolean {
  const ran = runCommand(scope, command);
  if (ran) notePointerCommand(scope, command);
  return ran;
}

/** Count key presses as keyboard use. Returns a cleanup. */
export function installKeyHints(target: Window = window): () => void {
  const onKeyDown = () => tracker.keyboard();
  target.addEventListener("keydown", onKeyDown, true);
  return () => target.removeEventListener("keydown", onKeyDown, true);
}
