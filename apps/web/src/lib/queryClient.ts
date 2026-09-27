import { QueryCache, QueryClient } from "@tanstack/react-query";

import { BridgeRequestError, UnauthorizedError } from "@/api/client";

interface QueryClientOptions {
  onUnauthorized?: () => void;
}

let activeClient: QueryClient | null = null;

/** Set by App.tsx (and tests) so action-registry runners can read cache state. */
export function setActiveQueryClient(client: QueryClient): void {
  activeClient = client;
}

export function getActiveQueryClient(): QueryClient | null {
  return activeClient;
}

/**
 * Retry what can change on its own (the daemon restarting, a timeout, a
 * 5xx), not answers that won't: a missing or malformed resource (4xx other
 * than 408 and 429) or an expired token. Retrying a not-found thread only
 * delayed its error by seconds.
 */
export function shouldRetryQuery(failureCount: number, err: unknown): boolean {
  if (err instanceof UnauthorizedError) return false;
  if (
    err instanceof BridgeRequestError &&
    err.status >= 400 &&
    err.status < 500 &&
    err.status !== 408 &&
    err.status !== 429
  ) {
    return false;
  }
  return failureCount < 2;
}

export function createQueryClient(options: QueryClientOptions = {}): QueryClient {
  return new QueryClient({
    queryCache: new QueryCache({
      onError: (err) => {
        if (err instanceof UnauthorizedError) options.onUnauthorized?.();
      },
    }),
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        gcTime: 5 * 60_000,
        refetchOnWindowFocus: false,
        retry: shouldRetryQuery,
      },
      mutations: {
        retry: false,
      },
    },
  });
}
