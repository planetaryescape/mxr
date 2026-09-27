/*
 * Every action in the app, registered once. Feature modules export arrays;
 * importing the registry barrel registers them as a side effect.
 */

import { toast } from "sonner";

import { accountsActions } from "@/features/accounts/actions";
import { analyticsActions } from "@/features/analytics/actions";
import { askActions } from "@/features/ask/actions";
import { composeActions } from "@/features/compose/actions";
import { diagnosticsActions } from "@/features/diagnostics/actions";
import { focusActions } from "@/features/focus/actions";
import { mailVerbActions } from "@/features/mail-actions/verbActions";
import { mailboxActions } from "@/features/mailbox/actions";
import { lensesFromShell, savedSearchLenses } from "@/features/mailbox/lenses";
import type { ShellResponse } from "@/features/mailbox/types";
import { placeActions } from "@/features/places/actions";
import { rulesActions } from "@/features/rules/actions";
import { screenerActions } from "@/features/screener/actions";
import { getActiveQueryClient } from "@/lib/queryClient";

import { navigationActions } from "./navigationActions";
import { listActions, readerActions, sidebarActions } from "./paneActions";
import { getRegistry } from "./registry";
import { getRuntimeNavigate } from "./runtime";
import { settingsActions } from "./settingsActions";
import type { Action } from "./types";

/** g 1 … g 9 open the Nth saved search, as in the TUI. */
const savedSearchJumps: Action[] = Array.from({ length: 9 }, (_, index) => ({
  id: `nav.saved-search-${index + 1}`,
  label: `Saved search ${index + 1}`,
  group: "Navigate" as const,
  shortcut: `g ${index + 1}`,
  hideInPalette: true,
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

const featureActions: Action[] = [
  ...navigationActions,
  ...savedSearchJumps,
  ...settingsActions,
  ...composeActions,
  ...mailVerbActions,
  ...mailboxActions,
  ...sidebarActions,
  ...listActions,
  ...readerActions,
  ...screenerActions,
  ...focusActions,
  ...placeActions,
  ...diagnosticsActions,
  ...rulesActions,
  ...accountsActions,
  ...analyticsActions,
  ...askActions,
];

let registered = false;

export function ensureCatalogRegistered(): void {
  if (registered) return;
  getRegistry().defineMany(featureActions);
  registered = true;
}

/** Test-only: re-register on the next call (use after `resetRegistry`). */
export function resetCatalogRegistration(): void {
  registered = false;
}

ensureCatalogRegistered();
