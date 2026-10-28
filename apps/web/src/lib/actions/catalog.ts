/*
 * Every action in the app, registered once. Feature modules export what
 * each action does; its label and keys come from the key definition
 * (`fromDefinition`). Importing the registry barrel registers them as a
 * side effect.
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

import { fromDefinition } from "./definition";
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

export function ensureCatalogRegistered(): void {
  if (registered) return;
  getRegistry().defineMany(featureActions.map(fromDefinition));
  registered = true;
}

/**
 * Rebuild every action from the definition as it stands now (after
 * `setServedKeys`). Throws, changing nothing, when the keys don't validate.
 */
export function reregisterCatalog(): void {
  getRegistry().replaceAll(featureActions.map(fromDefinition));
  registered = true;
}

/** Test-only: re-register on the next call (use after `resetRegistry`). */
export function resetCatalogRegistration(): void {
  registered = false;
}

ensureCatalogRegistered();
