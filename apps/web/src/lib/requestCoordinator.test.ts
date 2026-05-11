import { describe, expect, test } from "vitest";

import { WebRequestCoordinator } from "./requestCoordinator";

describe("WebRequestCoordinator", () => {
  test("aborts an older replaceable request with the same key", async () => {
    const coordinator = new WebRequestCoordinator();
    const first = coordinator.runReplaceable(
      "search",
      ({ signal }) =>
        new Promise<string>((_resolve, reject) => {
          signal.addEventListener(
            "abort",
            () => reject(new DOMException("aborted", "AbortError")),
            { once: true },
          );
        }),
    );

    const second = coordinator.runReplaceable("search", async () => "second");

    await expect(first).resolves.toEqual({ status: "aborted" });
    await expect(second).resolves.toEqual({ status: "committed", value: "second" });
  });

  test("serializes mutations", async () => {
    const coordinator = new WebRequestCoordinator();
    const order: string[] = [];
    const first = coordinator.enqueueMutation(async () => {
      order.push("first:start");
      await Promise.resolve();
      order.push("first:end");
    });
    const second = coordinator.enqueueMutation(async () => {
      order.push("second");
    });

    await Promise.all([first, second]);

    expect(order).toEqual(["first:start", "first:end", "second"]);
  });
});
