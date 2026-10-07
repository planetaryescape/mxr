import { renderHook } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { useAdvanceOnRemoval, type AdvanceOnRemoval } from "./useAdvanceOnRemoval";

type Props = Pick<AdvanceOnRemoval, "ids" | "selectedId" | "ready" | "isPresent">;

function setup(initial: Props) {
  const onAdvance = vi.fn<AdvanceOnRemoval["onAdvance"]>();
  const onEmpty = vi.fn<AdvanceOnRemoval["onEmpty"]>();
  const view = renderHook((props: Props) => useAdvanceOnRemoval({ ...props, onAdvance, onEmpty }), {
    initialProps: initial,
  });
  return { ...view, onAdvance, onEmpty };
}

describe("useAdvanceOnRemoval", () => {
  test("the open row leaving opens the next one, once", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    rerender({ ids: ["a", "c"], selectedId: "b" });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("c", "b");
    // A refetch before the new selection lands moves nothing again.
    rerender({ ids: ["a", "c"], selectedId: "b" });
    expect(onAdvance).toHaveBeenCalledOnce();
    expect(onEmpty).not.toHaveBeenCalled();
  });

  test("the last row goes to the previous one; an empty list empties", () => {
    const last = setup({ ids: ["a", "b"], selectedId: "b" });
    last.rerender({ ids: ["a"], selectedId: "b" });
    expect(last.onAdvance).toHaveBeenCalledWith("a", "b");

    const only = setup({ ids: ["a"], selectedId: "a" });
    only.rerender({ ids: [], selectedId: "a" });
    expect(only.onEmpty).toHaveBeenCalledExactlyOnceWith("a");
    expect(only.onAdvance).not.toHaveBeenCalled();
  });

  test("a selection the verb already moved is left alone", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    rerender({ ids: ["a", "b", "c"], selectedId: "c" });
    rerender({ ids: ["a", "c"], selectedId: "c" });
    expect(onAdvance).not.toHaveBeenCalled();
  });

  test("a row that was never listed (opened from a link) is not moved", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a", "b"], selectedId: "z" });
    rerender({ ids: ["a"], selectedId: "z" });
    expect(onAdvance).not.toHaveBeenCalled();
    expect(onEmpty).not.toHaveBeenCalled();
  });

  test("nothing moves while the list loads; a refetch mid-action compares with the last list", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    rerender({ ids: [], selectedId: "b", ready: false });
    expect(onEmpty).not.toHaveBeenCalled();
    rerender({ ids: ["a", "c"], selectedId: "b", ready: true });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("c", "b");
  });

  test("a row folded out of view but still present stays selected", () => {
    const present = new Set(["a", "b", "c"]);
    const { rerender, onAdvance } = setup({
      ids: ["a", "b", "c"],
      selectedId: "b",
      isPresent: (id) => present.has(id),
    });
    rerender({ ids: ["a", "c"], selectedId: "b", isPresent: (id) => present.has(id) });
    expect(onAdvance).not.toHaveBeenCalled();
  });

  test("the same row coming back and leaving again moves again (undo, then done)", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b"], selectedId: "a" });
    rerender({ ids: ["b"], selectedId: "a" });
    rerender({ ids: ["a", "b"], selectedId: "a" });
    rerender({ ids: ["b"], selectedId: "a" });
    expect(onAdvance).toHaveBeenCalledTimes(2);
  });

  test("undo bringing the row back reopens it", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    rerender({ ids: ["a", "c"], selectedId: "b" });
    expect(onAdvance).toHaveBeenLastCalledWith("c", "b");
    rerender({ ids: ["a", "c"], selectedId: "c" });
    rerender({ ids: ["a", "b", "c"], selectedId: "c" });
    expect(onAdvance).toHaveBeenLastCalledWith("b", "c");
  });

  test("undo reopens after a verb moved on by itself (the reader's archive)", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    // The reader moved to c and its row left in the same render.
    rerender({ ids: ["a", "c"], selectedId: "c" });
    expect(onAdvance).not.toHaveBeenCalled();
    rerender({ ids: ["a", "b", "c"], selectedId: "c" });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("b", "c");
  });

  test("undo after the list emptied reopens the row", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a"], selectedId: "a" });
    rerender({ ids: [], selectedId: "a" });
    expect(onEmpty).toHaveBeenCalledOnce();
    rerender({ ids: [], selectedId: null });
    rerender({ ids: ["a"], selectedId: null });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("a", null);
  });

  test("moving elsewhere after the done gives up the reopen", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    rerender({ ids: ["a", "c"], selectedId: "b" });
    rerender({ ids: ["a", "c"], selectedId: "c" });
    rerender({ ids: ["a", "c"], selectedId: "a" });
    rerender({ ids: ["a", "b", "c"], selectedId: "a" });
    expect(onAdvance).toHaveBeenCalledOnce();
  });

  test("an account switch closes rather than guessing", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a", "b"], selectedId: "a" });
    rerender({ ids: [], selectedId: "a", ready: false });
    rerender({ ids: ["x", "y"], selectedId: "a", ready: true });
    expect(onAdvance).not.toHaveBeenCalled();
    expect(onEmpty).toHaveBeenCalledExactlyOnceWith("a");
  });

  test("a selection that lands just after its row left still moves on (a click racing the verb)", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b", "c"], selectedId: null });
    rerender({ ids: ["a", "c"], selectedId: null });
    rerender({ ids: ["a", "c"], selectedId: "b" });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("c", "b");
  });
});
