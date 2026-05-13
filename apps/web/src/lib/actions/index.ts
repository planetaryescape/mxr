/*
 * Public surface of the action registry. Consumers (CommandPalette, keymap,
 * HelpDialog, StatusBar) import from here only.
 *
 * Importing this module triggers catalog registration as a side-effect.
 */

import "./catalog";

export { useActionContext } from "./context";
export { ensureCatalogRegistered, resetCatalogRegistration } from "./catalog";
export {
  actionShortcutSections,
  formatChord,
  useActionPrimaryHints,
  useActionsByGroup,
  useActionShortcutSections,
  useVisibleActions,
  type ShortcutHint,
  type ShortcutSection,
} from "./hints";
export { ActionRegistry, getRegistry, resetRegistry } from "./registry";
export { getRuntimeNavigate, setRuntimeNavigate } from "./runtime";
export type {
  Action,
  ActionContext,
  ActionGroup,
  ActionPredicate,
  ActionRunner,
  ShortcutChord,
} from "./types";
export {
  and,
  firstAccountOnly,
  not,
  onPane,
  onRoute,
  or,
  withFocusedMessage,
  withFocusedThread,
  withSelection,
} from "./when";
