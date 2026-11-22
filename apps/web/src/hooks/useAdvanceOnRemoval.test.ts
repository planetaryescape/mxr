import { renderHook } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { useAdvanceOnRemoval, type AdvanceOnRemoval } from "./useAdvanceOnRemoval";

type Props = Pick<
  AdvanceOnRemoval,
  "ids" | "selectedId" | "ready" | "isPresent" | "presenceSource"
>;

function setup(initial: Props) {
  const onAdvance = vi.fn<AdvanceOnRemoval["onAdvance"]>();
  const onEmpty = vi.fn<AdvanceOnRemoval["onEmpty"]>();
  const view = renderHook((props: Props) => useAdvanceOnRemoval({ ...props, onAdvance, onEmpty }), {
    initialProps: initial,
  });
  return { ...view, expectRemoval: view.result.current, onAdvance, onEmpty };
}

describe("useAdvanceOnRemoval", () => {
  test("the open row leaving opens the next one, once", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a", "b", "c"], selectedId: "b" });
    rerender({ ids: ["a", "c"], selectedId: "b" });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("c", "b");
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

  test("the same row coming back and leaving again moves again (undo, then done)", () => {
    const { rerender, onAdvance } = setup({ ids: ["a", "b"], selectedId: "a" });
    rerender({ ids: ["b"], selectedId: "a" });
    rerender({ ids: ["a", "b"], selectedId: "a" });
    rerender({ ids: ["b"], selectedId: "a" });
    expect(onAdvance).toHaveBeenCalledTimes(2);
  });

  test("a deferred middle-row archive advances in its requested direction", () => {
    const { rerender, expectRemoval, onAdvance } = setup({
      ids: ["a", "b", "c", "d"],
      selectedId: "c",
    });
    const release = expectRemoval("c", -1);

    rerender({ ids: ["a", "b", "d"], selectedId: "c" });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("b", "c");

    release();
    rerender({ ids: ["a", "b", "d"], selectedId: "b" });
    rerender({ ids: ["a", "b", "c", "d"], selectedId: "b" });
    expect(onAdvance).toHaveBeenLastCalledWith("c", "b");
  });

  test("undo after the list emptied reopens the row", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a"], selectedId: "a" });
    rerender({ ids: [], selectedId: "a" });
    expect(onEmpty).toHaveBeenCalledOnce();
    rerender({ ids: [], selectedId: null });
    rerender({ ids: ["a"], selectedId: null });
    expect(onAdvance).toHaveBeenCalledExactlyOnceWith("a", null);
  });

  test("an account switch closes rather than guessing", () => {
    const { rerender, onAdvance, onEmpty } = setup({ ids: ["a", "b"], selectedId: "a" });
    rerender({ ids: [], selectedId: "a", ready: false });
    rerender({ ids: ["x", "y"], selectedId: "a", ready: true });
    expect(onAdvance).not.toHaveBeenCalled();
    expect(onEmpty).toHaveBeenCalledExactlyOnceWith("a");
  });
});
