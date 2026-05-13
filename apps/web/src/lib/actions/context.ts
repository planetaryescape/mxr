/*
 * useActionContext — derives the synchronous portion of ActionContext from the
 * router and zustand stores. Account count is async (TanStack Query), so the
 * caller passes it in. This keeps the hook free of query mounts.
 */

import { useRouterState } from "@tanstack/react-router";
import { useMemo } from "react";

import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useSelection } from "@/state/selectionStore";

import type { ActionContext } from "./types";

interface ActionContextOverrides {
  accountCount?: number;
}

const THREAD_PATH_RE = /^\/m\/[^/]+\/[^/]+/;
const MESSAGE_PATH_RE = /^\/m\/[^/]+\/[^/]+\/[^/]+/;

export function useActionContext(overrides: ActionContextOverrides = {}): ActionContext {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const activePane = useMailboxPane((s) => s.activePane);
  const selectionCount = useSelection((s) => s.ids.size);
  const accountCount = overrides.accountCount ?? 0;

  return useMemo<ActionContext>(() => {
    return {
      path,
      activePane,
      selectionCount,
      accountCount,
      hasFocusedThread: THREAD_PATH_RE.test(path),
      hasFocusedMessage: MESSAGE_PATH_RE.test(path),
      isFirstAccountOnly: accountCount === 1,
    };
  }, [path, activePane, selectionCount, accountCount]);
}
