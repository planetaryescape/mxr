import { describe, expect, it } from "vitest";
import { DesktopRequestCoordinator } from "./requestCoordinator";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe("DesktopRequestCoordinator", () => {
  it("drops stale replaceable read results and only commits the newest one", async () => {
    const coordinator = new DesktopRequestCoordinator();
    const first = deferred<string>();
    const second = deferred<string>();

    const olderRequest = coordinator.runReplaceable(
      "search:mailbox",
      async () => await first.promise,
    );
    const newerRequest = coordinator.runReplaceable(
      "search:mailbox",
      async () => await second.promise,
    );

    first.resolve("older");
    second.resolve("newer");

    await expect(olderRequest).resolves.toEqual({ status: "stale" });
    await expect(newerRequest).resolves.toEqual({
      status: "committed",
      value: "newer",
    });
  });

  it("serializes queued mutations in submission order", async () => {
    const coordinator = new DesktopRequestCoordinator();
    const firstDone = deferred<void>();
    const order: string[] = [];

    const first = coordinator.enqueueMutation(async () => {
      order.push("start:first");
      await firstDone.promise;
      order.push("end:first");
      return "first";
    });
    const second = coordinator.enqueueMutation(async () => {
      order.push("start:second");
      order.push("end:second");
      return "second";
    });

    firstDone.resolve();

    await expect(first).resolves.toBe("first");
    await expect(second).resolves.toBe("second");
    expect(order).toEqual(["start:first", "end:first", "start:second", "end:second"]);
  });

  it("treats older compose work as stale once a newer draft operation is queued", async () => {
    const coordinator = new DesktopRequestCoordinator();
    const first = deferred<string>();
    const order: string[] = [];

    const olderDraft = coordinator.queueComposeLatest("compose:/tmp/draft.md", async () => {
      order.push("start:older");
      const value = await first.promise;
      order.push("end:older");
      return value;
    });
    const newerDraft = coordinator.queueComposeLatest("compose:/tmp/draft.md", async () => {
      order.push("start:newer");
      order.push("end:newer");
      return "newer";
    });

    await Promise.resolve();
    first.resolve("older");

    await expect(olderDraft).resolves.toEqual({ status: "stale" });
    await expect(newerDraft).resolves.toEqual({
      status: "committed",
      value: "newer",
    });
    expect(order).toEqual(["start:older", "end:older", "start:newer", "end:newer"]);
  });
});
