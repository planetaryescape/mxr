import { QueryCache, QueryClient } from "@tanstack/react-query";

import { BridgeRequestError, UnauthorizedError } from "@/api/client";

interface QueryClientOptions {
  onUnauthorized?: () => void;
}

let activeClient: QueryClient | null = null;

export function setActiveQueryClient(client: QueryClient): void {
  activeClient = client;
}

export function getActiveQueryClient(): QueryClient | null {
  return activeClient;
}

export function shouldRetryQuery(failureCount: number, err: unknown): boolean {
  if (err instanceof UnauthorizedError) return false;
  if (err instanceof BridgeRequestError && err.code === "offline_not_cached") return false;
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
        networkMode: "offlineFirst",
        staleTime: 30_000,
        gcTime: 5 * 60_000,
        refetchOnWindowFocus: false,
        retry: shouldRetryQuery,
      },
      mutations: {
        retry: false,
        networkMode: "always",
      },
    },
  });
}
