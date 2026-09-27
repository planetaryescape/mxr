/*
 * The one keydown listener. Resolves typed sequences against the action
 * registry using the active scopes, so a key fires exactly one action and
 * multi-key chords ("g i", "* a") work in every view.
 */

import type { ActionRegistry } from "@/lib/actions/registry";
import { invokeAction } from "@/lib/actions/registry";
import type { ActionContext, ActionScope } from "@/lib/actions/types";

import { isMacPlatform, tokenFromEvent, type KeyToken } from "./chord";

export const SEQUENCE_TIMEOUT_MS = 1000;

export interface DispatcherOptions {
  registry: ActionRegistry;
  context: () => ActionContext;
  /** Reports the pending sequence prefix ("g", "*") for the status bar. */
  onPendingChange?: (prefix: string | null) => void;
  /** Extra veto, e.g. while compose owns the page. */
  isSuspended?: () => boolean;
  mac?: boolean;
}

export function installKeyDispatcher(target: Window, options: DispatcherOptions): () => void {
  const mac = options.mac ?? isMacPlatform();
  // Off macOS, Ctrl is the Mod key, so a keypress parses as "Mod+d". TUI
  // chords written as "Ctrl+d" must still match there.
  // Browser essentials (Find, Print, Save, tabs) are never taken for a Ctrl
  // chord: on Windows and Linux they are the same keys.
  const spellings = (chord: string): string[] =>
    mac || !chord.includes("Mod+") || BROWSER_RESERVED.has(chord)
      ? [chord]
      : [chord, chord.replaceAll("Mod+", "Ctrl+")];
  const registry = {
    resolve: (chord: string, scopes: ActionScope[]) =>
      spellings(chord)
        .map((spelling) => options.registry.resolve(spelling, scopes))
        .find(Boolean),
    hasContinuation: (chord: string, scopes: ActionScope[]) =>
      spellings(chord).some((spelling) => options.registry.hasContinuation(spelling, scopes)),
  };
  let buffer: KeyToken[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;
  let deferred: (() => void) | null = null;

  const setPending = (tokens: KeyToken[]) => {
    buffer = tokens;
    options.onPendingChange?.(tokens.length > 0 ? tokens.join(" ") : null);
  };

  const clear = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    deferred = null;
    setPending([]);
  };

  const run = (scopes: ActionScope[], chord: string, ctx: ActionContext): boolean => {
    const binding = registry.resolve(chord, scopes);
    if (!binding) return false;
    invokeAction(binding.action, ctx);
    return true;
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.defaultPrevented || event.isComposing) return;
    const token = tokenFromEvent(event, mac);
    if (!token) return;
    if (ownsKeyboard(event.target) || options.isSuspended?.()) {
      // A global modifier chord (⌘K) is never text input, so it works from
      // a search box or a dialog that is still animating closed. Editing
      // chords stay with the field.
      const global =
        isModifierChord(token) && !EDITING_CHORDS.has(token)
          ? registry.resolve(token, ["global"])
          : undefined;
      if (!global || options.isSuspended?.()) return;
      event.preventDefault();
      clear();
      invokeAction(global.action, options.context());
      return;
    }
    if (activatesNativeControl(event.target, token)) return;

    const ctx = options.context();
    const scopes = ctx.scopes;
    const sequence = [...buffer, token];
    const chord = sequence.join(" ");
    const exact = registry.resolve(chord, scopes);
    const continues = registry.hasContinuation(chord, scopes);

    if (exact && !continues) {
      event.preventDefault();
      clear();
      invokeAction(exact.action, ctx);
      return;
    }
    if (continues) {
      event.preventDefault();
      if (timer !== null) clearTimeout(timer);
      setPending(sequence);
      // A chord that is complete but also a prefix waits, then runs.
      deferred = exact ? () => run(scopes, chord, options.context()) : null;
      timer = setTimeout(() => {
        const pendingRun = deferred;
        clear();
        pendingRun?.();
      }, SEQUENCE_TIMEOUT_MS);
      return;
    }
    if (buffer.length > 0) {
      // Escape only cancels a pending prefix, as in vim.
      if (token === "Escape") {
        event.preventDefault();
        clear();
        return;
      }
      // Dead end: drop the prefix and treat this key on its own, the way
      // vim does after a mistyped "g".
      clear();
      const single = registry.resolve(token, scopes);
      if (single && !registry.hasContinuation(token, scopes)) {
        event.preventDefault();
        invokeAction(single.action, ctx);
      } else if (registry.hasContinuation(token, scopes)) {
        event.preventDefault();
        setPending([token]);
        timer = setTimeout(clear, SEQUENCE_TIMEOUT_MS);
      }
    }
  };

  target.addEventListener("keydown", onKeyDown);
  return () => {
    target.removeEventListener("keydown", onKeyDown);
    clear();
  };
}

const BROWSER_RESERVED = new Set([
  "Mod+f",
  "Mod+p",
  "Mod+s",
  "Mod+w",
  "Mod+t",
  "Mod+n",
  "Mod+r",
  "Mod+l",
]);

function isModifierChord(token: KeyToken): boolean {
  return /^(Mod|Ctrl|Alt|Meta)\+/.test(token);
}

/** Chords a text field or editor owns; never stolen for app actions. */
const EDITING_CHORDS = new Set([
  "Mod+a",
  "Mod+c",
  "Mod+v",
  "Mod+x",
  "Mod+z",
  "Mod+Shift+z",
  "Mod+y",
  "Mod+b",
  "Mod+i",
  "Mod+u",
  "Mod+Enter",
  "Mod+s",
  "Mod+f",
  "Ctrl+a",
  "Ctrl+e",
  "Ctrl+k",
  "Ctrl+u",
  "Ctrl+w",
  "Ctrl+d",
  "Ctrl+h",
  // macOS Emacs-style cursor keys in every text field.
  "Ctrl+p",
  "Ctrl+n",
  "Ctrl+b",
  "Ctrl+f",
]);

/**
 * Text entry, open dialogs and menus handle their own keys. Radix portals
 * render dialogs/menus under `[role=dialog]`/`[role=menu]` and selects and
 * popovers inside a popper wrapper; cmdk lists sit under `[cmdk-root]`.
 * The mail list is a listbox too, but its keys belong to this dispatcher,
 * so listboxes are not vetoed by role.
 */
export function ownsKeyboard(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  // A dialog animating closed (Radix keeps it mounted with data-state
  // "closed") no longer owns anything; keys typed then belong to the page.
  const closing = target.closest(
    '[data-state="closed"][role="dialog"], [data-state="closed"][role="alertdialog"]',
  );
  if (closing) return false;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return (
    target.closest(
      'input, textarea, select, [contenteditable="true"], [role="dialog"], [role="alertdialog"], [role="menu"], [data-radix-popper-content-wrapper], [cmdk-root], [data-keys-owner]',
    ) !== null
  );
}

/** Enter/Space on a focused button, link or checkbox keeps its native meaning. */
function activatesNativeControl(target: EventTarget | null, token: KeyToken): boolean {
  if (token !== "Enter" && token !== "Space") return false;
  if (!(target instanceof Element)) return false;
  return (
    target.closest(
      'button, a[href], [role="button"], [role="checkbox"], [role="switch"], [role="tab"], summary',
    ) !== null
  );
}
