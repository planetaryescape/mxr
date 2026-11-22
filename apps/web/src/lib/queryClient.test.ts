import { onlineManager } from "@tanstack/react-query";
import { describe, expect, test } from "vitest";

import { createQueryClient } from "./queryClient";

describe("mutations while the daemon is down", () => {
  test("a scoped mutation runs (and fails) at once instead of waiting to replay", async () => {
    const client = createQueryClient();
    onlineManager.setOnline(false);
    try {
      let calls = 0;
      const mutation = client.getMutationCache().build(client, {
        scope: { id: "notification-chimes" },
        mutationFn: async () => {
          calls += 1;
          throw new Error("Couldn't reach mxr's daemon.");
        },
      });
      await expect(mutation.execute(undefined)).rejects.toThrow("Couldn't reach");
      expect(calls).toBe(1);
      expect(mutation.state.isPaused).toBe(false);
    } finally {
      onlineManager.setOnline(true);
    }
  });
});
