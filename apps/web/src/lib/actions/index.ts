/*
 * Public surface of the action registry. Importing this module registers
 * the catalog as a side effect.
 */

import "./catalog";

export { buildActionContext, snapshotActionContext, useActionContext } from "./context";
export { ensureCatalogRegistered, resetCatalogRegistration } from "./catalog";
export {
  formatChord,
  shortcutSections,
  useActionPrimaryHints,
  useActionsByGroup,
  useShortcutSections,
  useVisibleActions,
  type ShortcutHint,
  type ShortcutSection,
} from "./hints";
export {
  ActionRegistry,
  chordsOf,
  getRegistry,
  invokeAction,
  isAvailable,
  resetRegistry,
  scopesOf,
} from "./registry";
export { getRuntimeNavigate, setRuntimeNavigate } from "./runtime";
export type {
  Action,
  ActionContext,
  ActionGroup,
  ActionPredicate,
  ActionRunner,
  ActionScope,
  ShortcutChord,
} from "./types";
export {
  and,
  firstAccountOnly,
  not,
  onPane,
  onRoute,
  or,
  withFocusedThread,
  withSelection,
} from "./when";
