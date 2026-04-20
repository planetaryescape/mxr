import { useEffect, useRef } from "react";

export type ReplaceableRequestContext = {
  requestId: number;
  signal: AbortSignal;
};

export type ReplaceableResult<T> =
  | { status: "committed"; value: T }
  | { status: "aborted" | "stale" };

type ReplaceableEntry = {
  controller: AbortController;
  requestId: number;
};

type ComposeEntry = {
  requestId: number;
  tail: Promise<void>;
};

function isAbortError(error: unknown) {
  return error instanceof DOMException
    ? error.name === "AbortError"
    : error instanceof Error && error.name === "AbortError";
}

export class DesktopRequestCoordinator {
  private replaceable = new Map<string, ReplaceableEntry>();
  private mutationTail: Promise<void> = Promise.resolve();
  private compose = new Map<string, ComposeEntry>();

  async runReplaceable<T>(
    key: string,
    task: (context: ReplaceableRequestContext) => Promise<T>,
  ): Promise<ReplaceableResult<T>> {
    const previous = this.replaceable.get(key);
    previous?.controller.abort();

    const controller = new AbortController();
    const requestId = (previous?.requestId ?? 0) + 1;
    this.replaceable.set(key, { controller, requestId });

    try {
      const value = await task({ requestId, signal: controller.signal });
      const current = this.replaceable.get(key);
      if (!current || current.requestId !== requestId || current.controller !== controller) {
        return { status: "stale" };
      }
      return { status: "committed", value };
    } catch (error) {
      if (controller.signal.aborted || isAbortError(error)) {
        return { status: "aborted" };
      }
      throw error;
    } finally {
      const current = this.replaceable.get(key);
      if (current && current.requestId === requestId && current.controller === controller) {
        this.replaceable.delete(key);
      }
    }
  }

  async enqueueMutation<T>(task: () => Promise<T>): Promise<T> {
    const run = this.mutationTail.catch(() => undefined).then(task);
    this.mutationTail = run.then(
      () => undefined,
      () => undefined,
    );
    return await run;
  }

  async queueComposeLatest<T>(
    key: string,
    task: (requestId: number) => Promise<T>,
  ): Promise<ReplaceableResult<T>> {
    const entry = this.compose.get(key) ?? { requestId: 0, tail: Promise.resolve() };
    const requestId = entry.requestId + 1;
    entry.requestId = requestId;

    const run = entry.tail
      .catch(() => undefined)
      .then(async () => {
        const value = await task(requestId);
        const current = this.compose.get(key);
        if (!current || current.requestId !== requestId) {
          return { status: "stale" } satisfies ReplaceableResult<T>;
        }
        return { status: "committed", value } satisfies ReplaceableResult<T>;
      });

    entry.tail = run.then(
      () => undefined,
      () => undefined,
    );
    this.compose.set(key, entry);

    try {
      return await run;
    } finally {
      const current = this.compose.get(key);
      if (current && current.requestId === requestId) {
        await current.tail;
        const latest = this.compose.get(key);
        if (latest === current && latest.requestId === requestId) {
          this.compose.delete(key);
        }
      }
    }
  }

  async waitForMutationIdle(): Promise<void> {
    await this.mutationTail;
  }

  async waitForComposeIdle(key: string): Promise<void> {
    await this.compose.get(key)?.tail;
  }

  cancelAll() {
    for (const entry of this.replaceable.values()) {
      entry.controller.abort();
    }
    this.replaceable.clear();
  }
}

export function useDesktopRequestCoordinator() {
  const coordinatorRef = useRef<DesktopRequestCoordinator | null>(null);
  if (!coordinatorRef.current) {
    coordinatorRef.current = new DesktopRequestCoordinator();
  }

  useEffect(() => {
    const coordinator = coordinatorRef.current;
    return () => coordinator?.cancelAll();
  }, []);

  return coordinatorRef.current;
}
