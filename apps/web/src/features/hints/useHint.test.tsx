import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { modeGuideKey, type ModeGuide, type ModeId } from "@/features/modes/api";
import { setActiveQueryClient } from "@/lib/queryClient";

import type { Hint } from "./api";

type FetchOpts = Parameters<typeof import("@/api/client").apiFetch>[1];
interface ModeGuidesAnswer {
  kind: "ModeGuides";
  guides: ModeGuide[];
}
const apiFetch = vi.fn<(path: string, opts?: FetchOpts) => Promise<ModeGuidesAnswer>>();
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: (path: string, opts?: FetchOpts) => apiFetch(path, opts),
}));
let pathname = "/todo";
vi.mock("@tanstack/react-router", () => ({
  useRouterState: ({ select }: { select: (s: { location: { pathname: string } }) => string }) =>
    select({ location: { pathname } }),
}));

const { useHint } = await import("./useHint");
const { useHintSlot } = await import("./hintSlot");

function hint(id: string, seen = false): Hint {
  return { id, anchor: id, text: `About ${id}.`, key: { key: "e", verb: "done here" }, seen };
}

function guide(mode: ModeId, hints: Hint[]): ModeGuide {
  return {
    mode,
    name: mode,
    header: "",
    never_had_any: "",
    add_one: "",
    clear_for_now: "",
    lands_here: "",
    about: "",
    why_template: "",
    keys: [],
    first_run_line: "",
    hints,
  };
}

let qc: QueryClient;
function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

function press() {
  act(() => {
    window.history.replaceState(null, "", pathname);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "j" }));
  });
}

beforeEach(() => {
  pathname = "/todo";
  qc = new QueryClient();
  setActiveQueryClient(qc);
  qc.setQueryData(modeGuideKey("todo"), guide("todo", [hint("todo.runway"), hint("todo.catchup")]));
  useHintSlot.setState({ active: null, interactedOn: null, epoch: 0 });
  apiFetch.mockResolvedValue({ kind: "ModeGuides", guides: [] });
});

afterEach(() => {
  apiFetch.mockReset();
});

describe("useHint", () => {
  test("waits for a key or click on this page, then shows at its element", () => {
    const { result } = renderHook(() => useHint("todo", "todo.runway", { ready: true }), {
      wrapper,
    });
    expect(result.current.hint).toBeUndefined();
    press();
    expect(result.current.hint?.id).toBe("todo.runway");
  });
});
