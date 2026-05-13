import { QueryCache, QueryClient } from "@tanstack/react-query";

import { UnauthorizedError } from "@/api/client";

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
        retry: (failureCount, err) => {
          if (err instanceof UnauthorizedError) return false;
          return failureCount < 2;
        },
      },
      mutations: {
        retry: false,
      },
    },
  });
}
