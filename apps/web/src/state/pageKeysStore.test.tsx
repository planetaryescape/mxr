/* @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { buildActionContext, pageKeyHints } from "@/lib/actions";

import { usePageKeys, usePageKeysStore } from "./pageKeysStore";

function Page({ ids }: { ids: string[] }) {
  usePageKeys(ids);
  return null;
}

afterEach(() => usePageKeysStore.setState({ owner: null, ids: [] }));

describe("usePageKeys", () => {
  test("follows what is lit, and clears when nothing is", () => {
    const { rerender } = render(<Page ids={["todo.done", "todo.open-email"]} />);
    expect(usePageKeysStore.getState().ids).toEqual(["todo.done", "todo.open-email"]);
    rerender(<Page ids={["todo.primary"]} />);
    expect(usePageKeysStore.getState().ids).toEqual(["todo.primary"]);
    rerender(<Page ids={[]} />);
    expect(usePageKeysStore.getState().ids).toEqual([]);
  });

  test("a new array with the same ids does not write the store again", () => {
    const { rerender } = render(<Page ids={["todo.done"]} />);
    const first = usePageKeysStore.getState().ids;
    rerender(<Page ids={["todo.done"]} />);
    expect(usePageKeysStore.getState().ids).toBe(first);
  });

  test("clears when the page unmounts, and never clears another page's keys", () => {
    const first = render(<Page ids={["todo.done"]} />);
    render(<Page ids={["updates.peek"]} />);
    first.unmount();
    expect(usePageKeysStore.getState().ids).toEqual(["updates.peek"]);
  });
});

describe("pageKeyHints", () => {
  test("resolves each id to its key and short label, in the page's order, skipping unknown ids", () => {
    const hints = pageKeyHints(
      ["todo.done", "no.such.action", "todo.open-email"],
      buildActionContext({
        path: "/todo",
        activePane: "mailbox",
        scopeStack: ["todo"],
        selectionCount: 0,
        accountCount: 1,
        openThreadId: null,
      }),
    );
    expect(hints.map((hint) => [hint.keys[0], hint.shortLabel])).toEqual([
      ["Space", "Done"],
      ["o", "Email"],
    ]);
  });
});
