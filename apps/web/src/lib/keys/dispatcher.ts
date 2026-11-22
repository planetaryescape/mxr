import { definedKeys } from "@/lib/actions/definition";
import type { ActionRegistry, Binding } from "@/lib/actions/registry";
import { invokeAction } from "@/lib/actions/registry";
import type { Action, ActionContext, ActionScope } from "@/lib/actions/types";

import { isMacPlatform, tokenFromEvent, type KeyToken } from "./chord";
import { isCharacterKey } from "@/state/keymapStore";

import { isComposingKey } from "./composition";
import { focusActivePane } from "./focusPane";

export const SEQUENCE_TIMEOUT_MS = 4000;

export const REPEATABLE_ACTION_IDS: ReadonlySet<string> = new Set([
  "list.down",
  "list.up",
  "list.page-down",
  "list.page-up",
  "sidebar.down",
  "sidebar.up",
  "now.down",
  "now.up",
  "messages.down",
  "messages.up",
  "todo.down",
  "todo.up",
  "catchup.down",
  "catchup.up",
  "expired.down",
  "expired.up",
  "updates.down",
  "updates.up",
  "reading.down",
  "reading.up",
  "archive.down",
  "archive.up",
  "screener.down",
  "screener.up",
  "arrivals.down",
  "arrivals.up",
  "reader.scroll-down",
  "reader.scroll-up",
  "reader.page-down",
  "reader.page-up",
  "reader.next-message",
  "reader.prev-message",
]);

export interface DispatcherOptions {
  registry: ActionRegistry;
  context: () => ActionContext;
  onPendingChange?: (prefix: string | null) => void;
  onDeadEnd?: (sequence: string) => void;
  onMoved?: (action: Action, key: string | null) => void;
  isSuspended?: () => boolean;
  singleKeysOff?: () => boolean;
  mac?: boolean;
}

export function installKeyDispatcher(target: Window, options: DispatcherOptions): () => void {
  const mac = options.mac ?? isMacPlatform();
  const spellings = (chord: string): string[] =>
    mac || !chord.includes("Mod+") || BROWSER_RESERVED.has(chord)
      ? [chord]
      : [chord, chord.replaceAll("Mod+", "Ctrl+")];
  const registry = {
    resolve: (chord: string, scopes: ActionScope[]) =>
      spellings(chord)
        .map((spelling) => options.registry.resolve(spelling, scopes, mac))
        .find(Boolean),
    hasContinuation: (chord: string, scopes: ActionScope[]) =>
      spellings(chord).some((spelling) => options.registry.hasContinuation(spelling, scopes, mac)),
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

  const fire = (binding: Binding, ctx: ActionContext) => {
    if (binding.moved) {
      options.onMoved?.(binding.action, binding.movedTo ?? binding.action.shortcut ?? null);
    } else {
      invokeAction(binding.action, ctx, "key");
    }
  };

  const run = (scopes: ActionScope[], chord: string, ctx: ActionContext): boolean => {
    const binding = registry.resolve(chord, scopes);
    if (!binding) return false;
    fire(binding, ctx);
    return true;
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.defaultPrevented || isComposingKey(event)) return;
    const token = tokenFromEvent(event, mac);
    if (!token) return;
    if (ownsKeyboard(event.target) || options.isSuspended?.()) {
      clear();
      const closes = token !== "Escape" && definedKeys("overlay.close").includes(token);
      if (closes && closesOnQ(event.target)) {
        event.preventDefault();
        event.target.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
        );
        return;
      }
      const global =
        isModifierChord(token) && !EDITING_CHORDS.has(token)
          ? registry.resolve(token, ["global"])
          : undefined;
      if (!global || options.isSuspended?.()) return;
      event.preventDefault();
      if (event.repeat) return;
      clear();
      fire(global, options.context());
      return;
    }
    if (activatesNativeControl(event.target, token)) return;
    if (token === "Escape" && buffer.length === 0 && inChoiceControls(event.target)) {
      event.preventDefault();
      focusActivePane();
      return;
    }
    if (buffer.length === 0 && options.singleKeysOff?.() && isCharacterKey(token)) return;

    const ctx = options.context();
    const scopes = ctx.scopes;
    const sequence = [...buffer, token];
    const chord = sequence.join(" ");
    const exact = registry.resolve(chord, scopes);
    const continues = registry.hasContinuation(chord, scopes);

    if (event.repeat) {
      if (exact || continues) event.preventDefault();
      if (exact && !exact.moved && !continues && REPEATABLE_ACTION_IDS.has(exact.action.id)) {
        invokeAction(exact.action, ctx, "key");
      }
      return;
    }

    if (exact && !continues) {
      event.preventDefault();
      clear();
      fire(exact, ctx);
      return;
    }
    if (continues) {
      event.preventDefault();
      if (timer !== null) clearTimeout(timer);
      setPending(sequence);
      deferred = exact ? () => run(scopes, chord, options.context()) : null;
      timer = setTimeout(() => {
        const pendingRun = deferred;
        clear();
        pendingRun?.();
      }, SEQUENCE_TIMEOUT_MS);
      return;
    }
    if (buffer.length > 0) {
      if (token === "Escape") {
        event.preventDefault();
        clear();
        return;
      }
      clear();
      if (isModifierChord(token)) {
        const single = registry.resolve(token, scopes);
        if (single && !registry.hasContinuation(token, scopes)) {
          event.preventDefault();
          fire(single, ctx);
        }
        return;
      }
      event.preventDefault();
      options.onDeadEnd?.(chord);
    }
  };

  target.addEventListener("keydown", onKeyDown);
  const onFocusIn = (event: FocusEvent) => {
    if (ownsKeyboard(event.target)) clear();
  };
  target.addEventListener("focusin", onFocusIn);
  target.addEventListener("pointerdown", clear);
  target.addEventListener("blur", clear);
  return () => {
    target.removeEventListener("keydown", onKeyDown);
    target.removeEventListener("focusin", onFocusIn);
    target.removeEventListener("pointerdown", clear);
    target.removeEventListener("blur", clear);
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
  // A dialog or menu animating closed (Radix keeps it mounted, focus
  // inside, with data-state "closed") no longer owns anything; keys typed
  // then belong to the page.
  const closing = target.closest(
    '[data-state="closed"][role="dialog"], [data-state="closed"][role="alertdialog"], [data-state="closed"][role="menu"]',
  );
  if (closing) return false;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return (
    target.closest(
      'input, textarea, select, [contenteditable="true"], [role="dialog"], [role="alertdialog"], [role="menu"], [data-radix-popper-content-wrapper], [cmdk-root], [data-keys-owner]',
    ) !== null
  );
}

function closesOnQ(target: EventTarget | null): target is Element {
  if (!(target instanceof Element)) return false;
  if (target instanceof HTMLElement && target.isContentEditable) return false;
  if (
    target.closest(
      'input, textarea, select, [contenteditable="true"], [cmdk-root], [role="menu"], [data-compose-surface]',
    )
  )
    return false;
  return (
    target.closest(
      '[role="dialog"]:not([data-state="closed"]), [role="alertdialog"]:not([data-state="closed"])',
    ) !== null
  );
}

function inChoiceControls(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return (
    target.closest('[role="radiogroup"], [role="tablist"], [data-choice-controls]') !== null &&
    target.closest('[role="dialog"], [role="alertdialog"]') === null
  );
}

function activatesNativeControl(target: EventTarget | null, token: KeyToken): boolean {
  if (token !== "Enter" && token !== "Space") return false;
  if (!(target instanceof Element)) return false;
  return (
    target.closest(
      'button, a[href], [role="button"], [role="checkbox"], [role="switch"], [role="tab"], summary',
    ) !== null
  );
}
