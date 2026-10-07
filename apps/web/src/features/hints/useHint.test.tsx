import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { modeGuideKey, type ModeGuide, type ModeId } from "@/features/modes/api";
import type { ModeDoneOutcome } from "@/features/modes/modeDone";
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
const { withDoneHereHint } = await import("./doneHereHint");

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

/** A key press on the page the user is on now. */
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

  test("the key that navigated here was pressed on the page before, so it doesn't count", () => {
    useHintSlot.setState({ interactedOn: "now" });
    const { result } = renderHook(() => useHint("todo", "todo.runway", { ready: true }), {
      wrapper,
    });
    expect(result.current.hint).toBeUndefined();
  });

  test("the only thing on the page may show on arrival", () => {
    const { result } = renderHook(
      () => useHint("todo", "todo.runway", { ready: true, alone: true }),
      { wrapper },
    );
    expect(result.current.hint?.id).toBe("todo.runway");
  });

  test("one hint at a time, and dismissing it never reveals the next", () => {
    const { result, rerender } = renderHook(
      ({ catchupReady }: { catchupReady: boolean }) => ({
        runway: useHint("todo", "todo.runway", { ready: true }),
        catchup: useHint("todo", "todo.catchup", { ready: catchupReady }),
      }),
      { wrapper, initialProps: { catchupReady: true } },
    );
    press();
    expect(result.current.runway.hint?.id).toBe("todo.runway");
    expect(result.current.catchup.hint).toBeUndefined();

    act(() => result.current.runway.dismiss());
    expect(result.current.runway.hint).toBeUndefined();
    // No chained hint.
    expect(result.current.catchup.hint).toBeUndefined();

    // Its own element needed again, later: now it may show.
    rerender({ catchupReady: false });
    rerender({ catchupReady: true });
    expect(result.current.catchup.hint?.id).toBe("todo.catchup");
  });

  test("a seen hint never shows", () => {
    qc.setQueryData(modeGuideKey("todo"), guide("todo", [hint("todo.runway", true)]));
    const { result } = renderHook(
      () => useHint("todo", "todo.runway", { ready: true, alone: true }),
      { wrapper },
    );
    expect(result.current.hint).toBeUndefined();
  });

  test("dismissing posts SetHintSeen and marks it seen in every guide that carries it", async () => {
    qc.setQueryData(modeGuideKey("now"), guide("now", [hint("done_here")]));
    qc.setQueryData(modeGuideKey("messages"), guide("messages", [hint("done_here")]));
    pathname = "/now";
    const { result } = renderHook(() => useHint("now", "done_here", { ready: true, alone: true }), {
      wrapper,
    });
    expect(result.current.hint?.id).toBe("done_here");
    await act(async () => result.current.dismiss());
    expect(apiFetch).toHaveBeenCalledWith("/api/v1/mail/hints/done_here", {
      method: "POST",
      body: { seen: true },
    });
    for (const mode of ["now", "messages"] as const) {
      expect(qc.getQueryData<ModeGuide>(modeGuideKey(mode))?.hints[0]?.seen).toBe(true);
    }
    expect(result.current.hint).toBeUndefined();
  });

  test("a failed write brings the hint back for its next need", async () => {
    apiFetch.mockRejectedValue(new Error("daemon down"));
    const { result } = renderHook(
      () => useHint("todo", "todo.runway", { ready: true, alone: true }),
      { wrapper },
    );
    await act(async () => result.current.dismiss());
    expect(qc.getQueryData<ModeGuide>(modeGuideKey("todo"))?.hints[0]?.seen).toBe(false);
  });
});

/** The daemon's handoff copy, as markModeDone's default message reads it. */
function handoff(done: readonly ModeDoneOutcome[]): string {
  return done[0]?.copy ?? "";
}

describe("withDoneHereHint", () => {
  test("the first done here carries the hint in its toast, once", () => {
    qc.setQueryData(modeGuideKey("messages"), guide("messages", [hint("done_here")]));
    const done: ModeDoneOutcome[] = [
      { copy: "Done in Messages.", mode: "messages", provider: "Gmail", thread_id: "t" },
    ];
    expect(withDoneHereHint(handoff)(done)).toBe("Done in Messages. About done_here.");
    expect(apiFetch).toHaveBeenCalledWith("/api/v1/mail/hints/done_here", expect.anything());
    // Seen now, so later toasts are just the handoff.
    expect(withDoneHereHint(handoff)).toBe(handoff);
  });
});
