/*
 * "Next time" (blueprint 35, D171). When an action that has a key runs from
 * the palette or a pointer (toolbar, row, bulk bar and focus buttons), a
 * toast names its key: "Archive. Next time: e". It shows once per action
 * and profile: the daemon keeps that with the mode hints' seen state
 * (`SetHintSeen` on `next-time.<action id>`) and says whether this run was
 * the first, so the TUI and the web app never both teach the same key. A
 * key press never asks. At most one shows every few minutes; an action
 * skipped for that is offered again on its next run.
 */

import { toast } from "sonner";

import { postNextTimeSeen } from "@/features/hints/api";
import { formatChord } from "@/lib/keys/chord";
import { runCommand } from "@/lib/keys/controllers";
import { isSwitchedOff } from "@/state/keymapStore";

import { chordsOf, getRegistry, onActionInvoked, scopesOf } from "./registry";
import type { Action, ActionScope } from "./types";

/** At most one hint in this window, whatever it is for. */
export const HINT_GAP_MS = 5 * 60_000;

export interface KeyHint {
  actionId: string;
  keys: string;
  text: string;
}

export interface NextTimeDeps {
  /** True when this run is the first to mark the action's hint seen. */
  ask: (actionId: string) => Promise<boolean>;
  show: (hint: KeyHint) => void;
  now: () => number;
}

/**
 * The rule, free of the DOM and the daemon so it can be tested with a
 * clock. Call `offer` for each palette or pointer run.
 */
export function createNextTime({ ask, show, now }: NextTimeDeps) {
  const asked = new Set<string>();
  let lastShownAt = Number.NEGATIVE_INFINITY;
  let inFlight = false;
  return {
    async offer(action: Action, name?: string): Promise<void> {
      // With single-character shortcuts off, teach a key that still works.
      const keys = chordsOf(action).find((chord) => !isSwitchedOff(chord));
      if (!keys || action.paletteOnly || asked.has(action.id)) return;
      if (inFlight || now() - lastShownAt < HINT_GAP_MS) return;
      asked.add(action.id);
      inFlight = true;
      try {
        if (await ask(action.id)) {
          lastShownAt = now();
          show(hintFor(action, keys, name));
        }
      } catch {
        // Not stored, so not shown: the next run asks again.
        asked.delete(action.id);
      } finally {
        inFlight = false;
      }
    },
  };
}

/** "Archive. Next time: e" — `name` overrides the action's own words. */
export function hintFor(action: Action, chord: string, name?: string): KeyHint {
  const keys = formatChord(chord);
  const what = (name ?? action.shortLabel ?? action.label).replace(/…$/, "");
  return { actionId: action.id, keys, text: `${what}. Next time: ${keys}` };
}

const nextTime = createNextTime({
  ask: postNextTimeSeen,
  show: (hint) =>
    // Its own spot, bottom centre: never stacked over the action's undo.
    toast(hint.text, {
      id: "key-hint",
      duration: 8000,
      className: "key-hint",
      position: "bottom-center",
    }),
  now: () => Date.now(),
});

/**
 * Note a pointer use of the action with this id; may name its key. `name`
 * is what the button said, for actions whose own label is generic.
 */
export function notePointerUse(actionId: string, name?: string): void {
  const action = getRegistry().get(actionId);
  if (action) void nextTime.offer(action, name);
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
 * Run a scope command from a button, and offer that action's key. The
 * pointer twin of the key dispatcher's `invokeAction`.
 */
export function runPointerCommand(scope: ActionScope, command: string): boolean {
  const ran = runCommand(scope, command);
  if (ran) notePointerCommand(scope, command);
  return ran;
}

/** Offer the key for every action run from the palette or a pointer. Returns a cleanup. */
export function installKeyHints(): () => void {
  return onActionInvoked((action, source) => {
    if (source !== "key") void nextTime.offer(action);
  });
}
