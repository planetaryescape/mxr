import "./catalog";

export { buildActionContext, snapshotActionContext, useActionContext } from "./context";
export { ensureCatalogRegistered, loadActionCatalog, resetCatalogRegistration } from "./catalog";
export {
  formatChord,
  pageKeyHints,
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
