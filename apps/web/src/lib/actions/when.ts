import type { MailPane } from "@/state/mailboxPaneStore";

import type { ActionPredicate } from "./types";

export function onRoute(prefix: string): ActionPredicate {
  return (ctx) => ctx.path === prefix || ctx.path.startsWith(`${prefix}/`);
}

export function onPane(pane: MailPane): ActionPredicate {
  return (ctx) => ctx.activePane === pane;
}

export function withSelection(min = 1): ActionPredicate {
  return (ctx) => ctx.selectionCount >= min;
}

export function withFocusedThread(): ActionPredicate {
  return (ctx) => ctx.hasFocusedThread;
}

export function firstAccountOnly(): ActionPredicate {
  return (ctx) => ctx.accountCount === 1;
}

export function and(...preds: ActionPredicate[]): ActionPredicate {
  return (ctx) => {
    for (const p of preds) {
      if (!p(ctx)) return false;
    }
    return true;
  };
}

export function or(...preds: ActionPredicate[]): ActionPredicate {
  return (ctx) => {
    for (const p of preds) {
      if (p(ctx)) return true;
    }
    return false;
  };
}

export function not(pred: ActionPredicate): ActionPredicate {
  return (ctx) => !pred(ctx);
}
