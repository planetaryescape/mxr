/*
 * Every action in the app, registered once. Feature modules export what
 * each action does; its label and keys come from the key definition
 * (`fromDefinition`). The full definition loads after the first paint, so
 * `loadActionCatalog` is what puts them in the registry; importing the
 * barrel no longer does.
 */

import { toast } from "sonner";

import { accountsActions } from "@/features/accounts/actions";
import { analyticsActions } from "@/features/analytics/actions";
import { archiveActions } from "@/features/archive/actions";
import { arrivalActions } from "@/features/arrivals/actions";
import { askActions } from "@/features/ask/actions";
import { assistantActions } from "@/features/assistant/actions";
import { composeActions } from "@/features/compose/actions";
import { diagnosticsActions } from "@/features/diagnostics/actions";
import { focusActions } from "@/features/focus/actions";
import { mailVerbActions } from "@/features/mail-actions/verbActions";
import { mailboxActions } from "@/features/mailbox/actions";
import { lensesFromShell, savedSearchLenses } from "@/features/mailbox/lenses";
import { messagesActions } from "@/features/messages/actions";
import { nowActions } from "@/features/now/actions";
import type { ShellResponse } from "@/features/mailbox/types";
import { readingActions } from "@/features/reading/actions";
import { rulesActions } from "@/features/rules/actions";
import { screenerActions } from "@/features/screener/actions";
import { todoActions } from "@/features/todo/actions";
import { updatesActions } from "@/features/updates/actions";
import { getActiveQueryClient } from "@/lib/queryClient";

import { fromDefinition, isDefinitionLoaded, loadDefinition } from "./definition";
import { installActions } from "./installActions";
import { navigationActions } from "./navigationActions";
import { listActions, readerActions, sidebarActions } from "./paneActions";
import { getRegistry } from "./registry";
import { getRuntimeNavigate } from "./runtime";
import { settingsActions } from "./settingsActions";
import type { ActionBehavior } from "./types";

/** g 1 … g 9 open the Nth saved search, as in the TUI. */
const savedSearchJumps: ActionBehavior[] = Array.from({ length: 9 }, (_, index) => ({
  id: `nav.saved-search-${index + 1}`,
  run: () => {
    const shell = getActiveQueryClient()?.getQueryData<ShellResponse>(["shell"]);
    const lens = savedSearchLenses(lensesFromShell(shell))[index];
    if (!lens) {
      toast.info(`No saved search in slot ${index + 1}`);
      return;
    }
    getRuntimeNavigate().navigate(lens.path);
  },
}));

const featureActions: ActionBehavior[] = [
  ...navigationActions,
  ...savedSearchJumps,
  ...settingsActions,
  ...installActions,
  ...composeActions,
  ...mailVerbActions,
  ...mailboxActions,
  ...sidebarActions,
  ...listActions,
  ...readerActions,
  ...screenerActions,
  ...focusActions,
  ...todoActions,
  ...archiveActions,
  ...readingActions,
  ...nowActions,
  ...messagesActions,
  ...updatesActions,
  ...arrivalActions,
  ...diagnosticsActions,
  ...rulesActions,
  ...accountsActions,
  ...analyticsActions,
  ...askActions,
  ...assistantActions,
];

let registered = false;

/**
 * Register every action. The full definition arrives after the first paint,
 * so this waits for it (`loadActionCatalog`); before that a key runs nothing
 * and the strip is empty rather than wrong.
 */
export function ensureCatalogRegistered(): void {
  if (registered) return;
  if (!isDefinitionLoaded()) {
    throw new Error("mxr: the action catalog has not loaded yet; await loadActionCatalog() first");
  }
  getRegistry().defineMany(featureActions.map(fromDefinition));
  registered = true;
}

/**
 * Fetch the full definition and register the actions. Called after the first
 * contentful paint by `servedKeymap`; keys are live when it resolves.
 */
export async function loadActionCatalog(): Promise<void> {
  await loadDefinition();
  reregisterCatalog();
}

/**
 * Rebuild every action from the definition as it stands now (after
 * `setServedKeys`). Throws, changing nothing, when the keys don't validate.
 */
export function reregisterCatalog(): void {
  if (!isDefinitionLoaded()) {
    throw new Error("mxr: the action catalog has not loaded yet; await loadActionCatalog() first");
  }
  getRegistry().replaceAll(featureActions.map(fromDefinition));
  registered = true;
}

/** Test-only: re-register on the next call (use after `resetRegistry`). */
export function resetCatalogRegistration(): void {
  registered = false;
}

// A test imports the barrel and expects actions at once, while the app waits
// for the catalog after the first paint. `definition.ts` primes the catalog
// from the same generated module for tests, so register now.
if (import.meta.env.MODE === "test") {
  ensureCatalogRegistered();
}
