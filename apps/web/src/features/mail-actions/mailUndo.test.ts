import { beforeEach, describe, expect, it, vi } from "vitest";

const undoMutation = vi.fn<(id: string) => Promise<void>>();
vi.mock("@/features/mailbox/api", () => ({
  undoMutation: (id: string) => undoMutation(id),
  unsnoozeMessage: vi.fn<() => Promise<void>>(),
}));
vi.mock("./mailQueryInvalidation", () => ({ invalidateMailQueries: vi.fn<() => Promise<void>>(async () => {}) }));
const toastError = vi.fn<(...args: unknown[]) => void>();
vi.mock("sonner", () => ({
  toast: Object.assign(vi.fn<() => void>(), { success: vi.fn<() => void>(), error: (...args: unknown[]) => toastError(...args), dismiss: vi.fn<() => void>() }),
}));

import { useUndo } from "@/state/undoStore";
import { performUndo } from "./mailUndo";

describe("a partly failed undo", () => {
  beforeEach(() => {
    undoMutation.mockReset();
    toastError.mockReset();
  });

  it("offers u and a Retry that undo the same id again", async () => {
    undoMutation.mockRejectedValueOnce(
      new Error("undo: partly done: restored 3, 1 failed and can be retried with the same undo (x)"),
    );
    expect(await performUndo("m-1")).toBe(false);
    expect(toastError).toHaveBeenCalledWith(
      "Some messages weren't restored",
      expect.objectContaining({ action: expect.objectContaining({ label: "Retry" }) }),
    );

    undoMutation.mockResolvedValueOnce(undefined);
    const retry = useUndo.getState().lastUndo;
    expect(retry).toBeTypeOf("function");
    expect(await retry?.()).toBe(true);
    expect(undoMutation).toHaveBeenLastCalledWith("m-1");
  });

  it("does not offer a retry for an expired undo", async () => {
    undoMutation.mockRejectedValueOnce(new Error("undo: window expired for mutation `m-2`"));
    await performUndo("m-2");
    expect(toastError).toHaveBeenCalledWith("Undo failed", expect.anything());
  });
});
