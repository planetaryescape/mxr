import { describe, expect, test } from "vitest";

import { WebRequestCoordinator } from "./requestCoordinator";

describe("WebRequestCoordinator", () => {
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
