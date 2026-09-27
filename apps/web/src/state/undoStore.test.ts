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
    useUndo.getState().recordNoUndo(); // e.g. a star after the archive

    expect(runLatestUndo()).toBeNull();
    expect(undoArchive).not.toHaveBeenCalled();
  });

  test("the newest of a pending send and a mail change wins", () => {
    vi.useFakeTimers();
    const cancelSend = vi.fn<() => void>();
    const undoArchive = vi.fn<() => Promise<boolean>>(async () => true);
    vi.setSystemTime(1000);
    useUndo.getState().setPendingSendCancel(cancelSend);
    vi.setSystemTime(2000);
    useUndo.getState().recordUndo(undoArchive, "mut-archive");

    expect(runLatestUndo()).toBe("mail");
    expect(undoArchive).toHaveBeenCalledTimes(1);
    expect(cancelSend).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  test("an older toast's undo cannot clear a newer undo", () => {
    const older = vi.fn<() => Promise<boolean>>(async () => true);
    const newer = vi.fn<() => Promise<boolean>>(async () => true);
    useUndo.getState().recordUndo(older);
    useUndo.getState().recordUndo(newer);
    useUndo.getState().retireUndo(older);

    expect(useUndo.getState().lastUndo).toBe(newer);
  });
});
