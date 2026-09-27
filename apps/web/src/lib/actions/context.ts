/*
 * ActionContext: what the palette, help and key dispatcher need to decide
 * which actions apply. One builder serves both the React hook and the
 * dispatcher's snapshot at keypress time.
 */

import { useRouterState } from "@tanstack/react-router";
import { useMemo } from "react";

import { getActiveQueryClient } from "@/lib/queryClient";
import { useKeyScope } from "@/state/keyScopeStore";
import { useMailboxPane, type MailPane } from "@/state/mailboxPaneStore";
import { useOpenThread } from "@/state/openThreadStore";
import { useSelection } from "@/state/selectionStore";

import type { ActionContext, ActionScope } from "./types";

interface ContextInputs {
  path: string;
  activePane: MailPane;
  scopeStack: readonly ActionScope[];
  selectionCount: number;
  accountCount: number;
  openThreadId: string | null;
}

export function buildActionContext(inputs: ContextInputs): ActionContext {
  const scopes: ActionScope[] = inputs.scopeStack
    .toReversed()
    .filter((scope) => scope !== "global");
  scopes.push("global");
  return {
    path: inputs.path,
    activePane: inputs.activePane,
    scopes: [...new Set(scopes)],
    selectionCount: inputs.selectionCount,
    accountCount: inputs.accountCount,
    hasFocusedThread: inputs.openThreadId !== null,
    isFirstAccountOnly: inputs.accountCount === 1,
  };
}

function cachedAccountCount(): number {
  const data = getActiveQueryClient()?.getQueryData<{ accounts?: unknown[] }>(["accounts"]);
  return data?.accounts?.length ?? 0;
}

/** Context at this instant, for code outside React (the key dispatcher). */
export function snapshotActionContext(): ActionContext {
  return buildActionContext({
    path: typeof window === "undefined" ? "/" : window.location.pathname,
    activePane: useMailboxPane.getState().activePane,
    scopeStack: useKeyScope.getState().stack,
    selectionCount: useSelection.getState().ids.size,
    accountCount: cachedAccountCount(),
    openThreadId: useOpenThread.getState().threadId,
  });
}

export function useActionContext(overrides: { accountCount?: number } = {}): ActionContext {
  const path = useRouterState({ select: (state) => state.location.pathname });
  const activePane = useMailboxPane((state) => state.activePane);
  const scopeStack = useKeyScope((state) => state.stack);
  const selectionCount = useSelection((state) => state.ids.size);
  const openThreadId = useOpenThread((state) => state.threadId);
  const accountCount = overrides.accountCount ?? cachedAccountCount();
  return useMemo(
    () =>
      buildActionContext({
        path,
        activePane,
        scopeStack,
        selectionCount,
        accountCount,
        openThreadId,
      }),
    [path, activePane, scopeStack, selectionCount, accountCount, openThreadId],
  );
}
