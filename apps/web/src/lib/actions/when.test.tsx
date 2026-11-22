/* @vitest-environment node */

import { describe, expect, test, vi } from "vitest";

import type { ActionContext } from "./types";
import { and, onRoute, or, withSelection } from "./when";

const baseCtx: ActionContext = {
  path: "/m/inbox",
  activePane: "mailbox",
  scopes: ["global"],
  selectionCount: 0,
  accountCount: 1,
  hasFocusedThread: false,
  isFirstAccountOnly: true,
};

describe("when predicates", () => {
  test("and short-circuits on first false predicate", () => {
    const second = vi.fn<() => boolean>(() => true);
    const pred = and(() => false, second);
    expect(pred(baseCtx)).toBe(false);
    expect(second).not.toHaveBeenCalled();
  });

  test("onRoute matches prefix not substring (/m/inbox-archived should not match /m/inbox)", () => {
    const pred = onRoute("/m/inbox");
    expect(pred({ ...baseCtx, path: "/m/inbox" })).toBe(true);
    expect(pred({ ...baseCtx, path: "/m/inbox/thread-1" })).toBe(true);
    expect(pred({ ...baseCtx, path: "/m/inbox-archived" })).toBe(false);
  });

  test("or returns true when any predicate matches", () => {
    const pred = or(
      () => false,
      () => true,
      () => false,
    );
    expect(pred(baseCtx)).toBe(true);
    expect(
      or(
        () => false,
        () => false,
      )(baseCtx),
    ).toBe(false);
  });

  test("withSelection enforces minimum selected count", () => {
    expect(withSelection()({ ...baseCtx, selectionCount: 0 })).toBe(false);
    expect(withSelection()({ ...baseCtx, selectionCount: 1 })).toBe(true);
    expect(withSelection(3)({ ...baseCtx, selectionCount: 2 })).toBe(false);
    expect(withSelection(3)({ ...baseCtx, selectionCount: 3 })).toBe(true);
  });
});
