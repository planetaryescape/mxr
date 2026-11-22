/* @vitest-environment node */

import { beforeEach, describe, expect, test, vi } from "vitest";

import { runLatestUndo, useUndo } from "./undoStore";

beforeEach(() => {
  useUndo.setState({
    lastUndo: null,
    lastUndoAt: 0,
    lastMutationId: null,
    pendingSendCancel: null,
    pendingSendAt: 0,
  });
});

describe("undo ordering", () => {
  test("a newer change without undo stops u reaching back to an older archive", () => {
    const undoArchive = vi.fn<() => Promise<boolean>>(async () => true);
    useUndo.getState().recordUndo(undoArchive, "mut-archive");
    useUndo.getState().recordNoUndo();

    expect(runLatestUndo()).toBeNull();
    expect(undoArchive).not.toHaveBeenCalled();
  });
});
