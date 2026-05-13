/* @vitest-environment jsdom */

import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, test } from "vitest";

import { setActiveQueryClient } from "@/lib/queryClient";

import { diagnosticsActions } from "./actions";

const baseCtx = {
  path: "/m/inbox",
  activePane: "mailbox" as const,
  selectionCount: 0,
  accountCount: 0,
  hasFocusedThread: false,
  hasFocusedMessage: false,
  isFirstAccountOnly: false,
};

describe("diagnostics actions enable/disable predicates", () => {
  let client: QueryClient;

  beforeEach(() => {
    client = new QueryClient();
    setActiveQueryClient(client);
  });

  afterEach(() => {
    client.clear();
  });

  test("semantic.enable is visible when the cache says disabled", () => {
    client.setQueryData(["diagnostics", "semantic"], {
      status: { enabled: false, profiles: [] },
    });

    const enable = diagnosticsActions.find((a) => a.id === "semantic.enable");
    const disable = diagnosticsActions.find((a) => a.id === "semantic.disable");

    expect(enable?.when?.(baseCtx)).toBe(true);
    expect(disable?.when?.(baseCtx)).toBe(false);
  });

  test("semantic.disable is visible when the cache says enabled", () => {
    client.setQueryData(["diagnostics", "semantic"], {
      status: { enabled: true, profiles: [] },
    });

    const enable = diagnosticsActions.find((a) => a.id === "semantic.enable");
    const disable = diagnosticsActions.find((a) => a.id === "semantic.disable");

    expect(enable?.when?.(baseCtx)).toBe(false);
    expect(disable?.when?.(baseCtx)).toBe(true);
  });

  test("semantic.enable falls back to enabled=false when cache is empty", () => {
    const enable = diagnosticsActions.find((a) => a.id === "semantic.enable");
    expect(enable?.when?.(baseCtx)).toBe(true);
  });
});
