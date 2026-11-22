import { toast } from "sonner";

import { postNextTimeSeen } from "@/features/hints/api";
import { formatChord } from "@/lib/keys/chord";
import { runCommand } from "@/lib/keys/controllers";
import { isSwitchedOff } from "@/state/keymapStore";

import { chordsOf, getRegistry, onActionInvoked, scopesOf } from "./registry";
import type { Action, ActionScope } from "./types";

export const HINT_GAP_MS = 5 * 60_000;

export interface KeyHint {
  actionId: string;
  keys: string;
  text: string;
}

export interface NextTimeDeps {
  ask: (actionId: string) => Promise<boolean>;
  show: (hint: KeyHint) => void;
  now: () => number;
}

export function createNextTime({ ask, show, now }: NextTimeDeps) {
  const asked = new Set<string>();
  let lastShownAt = Number.NEGATIVE_INFINITY;
  let inFlight = false;
  return {
    async offer(action: Action, name?: string): Promise<void> {
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
        asked.delete(action.id);
      } finally {
        inFlight = false;
      }
    },
  };
}

export function hintFor(action: Action, chord: string, name?: string): KeyHint {
  const keys = formatChord(chord);
  const what = (name ?? action.shortLabel ?? action.label).replace(/…$/, "");
  return { actionId: action.id, keys, text: `${what}. Next time: ${keys}` };
}

const nextTime = createNextTime({
  ask: postNextTimeSeen,
  show: (hint) =>
    toast(hint.text, {
      id: "key-hint",
      duration: 8000,
      className: "key-hint",
      position: "bottom-center",
    }),
  now: () => Date.now(),
});

export function notePointerUse(actionId: string, name?: string): void {
  const action = getRegistry().get(actionId);
  if (action) void nextTime.offer(action, name);
}

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

export function runPointerCommand(scope: ActionScope, command: string): boolean {
  const ran = runCommand(scope, command);
  if (ran) notePointerCommand(scope, command);
  return ran;
}

export function installKeyHints(): () => void {
  return onActionInvoked((action, source) => {
    if (source !== "key") void nextTime.offer(action);
  });
}
