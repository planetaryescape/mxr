/*
 * Aggregated catalog of every action in the app. Per-feature `actions.ts`
 * modules export arrays that get registered here. Auto-registers the first
 * time anything imports from the registry barrel.
 */

import { accountsActions } from "@/features/accounts/actions";
import { composeActions } from "@/features/compose/actions";
import { diagnosticsActions } from "@/features/diagnostics/actions";
import { rulesActions } from "@/features/rules/actions";

import { navigationActions } from "./navigationActions";
import { getRegistry } from "./registry";
import { settingsActions } from "./settingsActions";
import type { Action } from "./types";

const featureActions: Action[] = [
  ...navigationActions,
  ...settingsActions,
  ...composeActions,
  ...diagnosticsActions,
  ...rulesActions,
  ...accountsActions,
];

let registered = false;

export function ensureCatalogRegistered(): void {
  if (registered) return;
  const reg = getRegistry();
  reg.defineMany(featureActions);
  registered = true;
}

/** Test-only — re-register on the next call (use after `resetRegistry`). */
export function resetCatalogRegistration(): void {
  registered = false;
}

ensureCatalogRegistered();
