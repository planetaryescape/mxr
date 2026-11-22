import { beforeEach, describe, expect, test, vi } from "vitest";

import { useUndo } from "@/state/undoStore";

import { markModeDone } from "./modeDone";

const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

const todoDone = (id: string) => ({
  kind: "ModeDone",
  dry_run: false,
  mutation_id: id,
  items: [{ thread_id: "shared", mode: "todo", provider: "Gmail", copy: "Ticked off." }],
});

describe("done here when nothing was done", () => {
  test("keeps the daemon's undo when it changed something part way", async () => {
    fetchMock.mockResolvedValueOnce({
      kind: "ModeDone",
      dry_run: false,
      mutation_id: "m-partial",
      items: [
        {
          thread_id: "t1",
          mode: "messages",
          provider: "Gmail",
          copy: "",
          error: "the mail server refused; run done again to retry",
        },
      ],
    });
    const done = await markModeDone("messages", ["t1"]);
    expect(done).toBe(false);
    expect(useUndo.getState().lastMutationId).toBe("m-partial");
    expect(useUndo.getState().lastUndo).not.toBeNull();
  });
});

describe("named To do completion in flight", () => {
  beforeEach(() => {
    fetchMock.mockReset();
    useUndo.getState().clear();
  });

  test("a duplicate activation sends once and preserves the successful Undo", async () => {
    let finish!: (value: ReturnType<typeof todoDone>) => void;
    fetchMock.mockImplementationOnce(
      () => new Promise<ReturnType<typeof todoDone>>((resolve) => (finish = resolve)),
    );
    fetchMock.mockResolvedValueOnce({
      kind: "ModeDone",
      dry_run: false,
      items: [{ thread_id: "shared", mode: "todo", error: "no open to-do on this conversation" }],
    });
    const first = markModeDone("todo", ["shared"], { todoIds: ["one"] });
    await markModeDone("todo", ["shared"], { todoIds: ["one"] });
    finish(todoDone("undo-one"));
    await first;
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(useUndo.getState().lastMutationId).toBe("undo-one");
    expect(useUndo.getState().lastUndo).toBeTypeOf("function");
  });
});
