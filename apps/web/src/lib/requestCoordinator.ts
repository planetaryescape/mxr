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

export class WebRequestCoordinator {
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
      if (controller.signal.aborted || isAbortError(error)) return { status: "aborted" };
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
        if (!current || current.requestId !== requestId) return { status: "stale" } as const;
        return { status: "committed", value } as const;
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
        if (latest === current && latest.requestId === requestId) this.compose.delete(key);
      }
    }
  }

  cancelAll(): void {
    for (const entry of this.replaceable.values()) entry.controller.abort();
    this.replaceable.clear();
  }
}

export const requestCoordinator = new WebRequestCoordinator();

export async function runReplaceableQuery<T>(
  key: string,
  reactQuerySignal: AbortSignal,
  task: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
  const result = await requestCoordinator.runReplaceable(key, ({ signal }) =>
    task(combineAbortSignals([reactQuerySignal, signal])),
  );
  if (result.status === "committed") return result.value;
  throw abortError(`Request ${result.status}`);
}

function combineAbortSignals(signals: AbortSignal[]): AbortSignal {
  const abortSignal = AbortSignal as typeof AbortSignal & {
    any?: (signals: AbortSignal[]) => AbortSignal;
  };
  if (typeof abortSignal.any === "function") return abortSignal.any(signals);

  const controller = new AbortController();
  const abort = () => controller.abort();
  for (const signal of signals) {
    if (signal.aborted) {
      controller.abort();
      break;
    }
    signal.addEventListener("abort", abort, { once: true });
  }
  return controller.signal;
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException
    ? error.name === "AbortError"
    : error instanceof Error && error.name === "AbortError";
}

function abortError(message: string): Error {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}
