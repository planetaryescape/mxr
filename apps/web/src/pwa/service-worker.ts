/// <reference lib="webworker" />

import { clientsClaim, type WorkboxPlugin } from "workbox-core";
import { ExpirationPlugin } from "workbox-expiration";
import { cleanupOutdatedCaches, matchPrecache, precacheAndRoute } from "workbox-precaching";
import { NavigationRoute, registerRoute, setCatchHandler } from "workbox-routing";
import { CacheFirst, NetworkFirst, NetworkOnly } from "workbox-strategies";

import {
  isOfflineRead,
  isUnavailableResponse,
  mayCacheMail,
  OFFLINE_HEADER,
  OFFLINE_MAIL_CACHE,
} from "./offlineReads";

declare const self: ServiceWorkerGlobalScope & {
  __WB_MANIFEST: Array<{ url: string; revision?: string | null }>;
};

clientsClaim();
cleanupOutdatedCaches();
// oxlint-disable-next-line no-underscore-dangle -- Workbox replaces this exact build-time marker.
precacheAndRoute(self.__WB_MANIFEST);

let cacheGeneration = 0;
async function clearMail(): Promise<void> {
  cacheGeneration += 1;
  await caches.delete(OFFLINE_MAIL_CACHE);
}

self.addEventListener("message", (event: ExtendableMessageEvent) => {
  if (event.data?.type === "SKIP_WAITING") event.waitUntil(self.skipWaiting());
  if (event.data?.type === "CLEAR_OFFLINE_MAIL") event.waitUntil(clearMail());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((name) => name.startsWith("mxr-opened-mail-") && name !== OFFLINE_MAIL_CACHE)
            .map((name) => caches.delete(name)),
        ),
      ),
  );
});

const requireConnection: WorkboxPlugin = {
  fetchDidSucceed: async ({ response }) => {
    if (isUnavailableResponse(response)) throw new Error("Mail host unavailable");
    return response;
  },
};

const privateReads: WorkboxPlugin = {
  // Cache only the read URL, never its bearer-token request headers.
  cacheKeyWillBeUsed: async ({ request }) => request.url,
  handlerWillStart: async ({ state }) => {
    if (state) state.generation = cacheGeneration;
  },
  fetchDidSucceed: async ({ response }) => {
    if (isUnavailableResponse(response)) throw new Error("Mail host unavailable");
    if (response.status === 401 || response.status === 403) {
      await clearMail();
      // A slow authority response may arrive after NetworkFirst used offline data.
      if (response.status === 401) {
        for (const client of await self.clients.matchAll({ type: "window" })) {
          // oxlint-disable-next-line unicorn/require-post-message-target-origin -- Service-worker Client.postMessage has no targetOrigin parameter.
          client.postMessage({ type: "MXR_SESSION_REJECTED" });
        }
      }
    }
    return response;
  },
  cacheWillUpdate: async ({ request, response, state }) => {
    if (!state || state.generation !== cacheGeneration || !mayCacheMail(response)) return null;
    const cache = await caches.open(OFFLINE_MAIL_CACHE);
    const previous = await cache.match(request.url);
    const owner = response.headers.get("x-mxr-workspace");
    if (previous && owner !== previous.headers.get("x-mxr-workspace")) {
      await clearMail();
      state.generation = cacheGeneration;
    }
    return state.generation === cacheGeneration ? response : null;
  },
  handlerDidComplete: async ({ state }) => {
    // A read finishing after sign-out must not recreate the private cache.
    if (state && state.generation !== cacheGeneration) await caches.delete(OFFLINE_MAIL_CACHE);
  },
  cachedResponseWillBeUsed: async ({ cachedResponse }) => {
    if (!cachedResponse) return undefined;
    const headers = new Headers(cachedResponse.headers);
    headers.set(OFFLINE_HEADER, "1");
    return new Response(cachedResponse.body, {
      status: cachedResponse.status,
      headers,
    });
  },
};

registerRoute(
  ({ url, request }) => isOfflineRead(url, request.method, self.location.origin),
  new NetworkFirst({
    cacheName: OFFLINE_MAIL_CACHE,
    networkTimeoutSeconds: 4,
    plugins: [
      privateReads,
      new ExpirationPlugin({
        maxEntries: 200,
        maxAgeSeconds: 30 * 24 * 60 * 60,
        purgeOnQuotaError: true,
      }),
    ],
  }),
);

// All other API reads stay online-only; an offline miss is a useful JSON error.
registerRoute(
  ({ url }) =>
    url.origin === self.location.origin &&
    ["/api/v1/health", "/api/v1/admin/status", "/api/v1/auth/local-token"].includes(url.pathname),
  new NetworkOnly({ networkTimeoutSeconds: 4, plugins: [requireConnection] }),
);

registerRoute(
  ({ url }) => url.origin === self.location.origin && url.pathname.startsWith("/api/"),
  new NetworkOnly({ plugins: [requireConnection] }),
);

for (const method of ["POST", "PUT", "PATCH", "DELETE"] as const) {
  registerRoute(
    ({ url }) => url.origin === self.location.origin && url.pathname.startsWith("/api/"),
    new NetworkOnly({ plugins: [requireConnection] }),
    method,
  );
}

registerRoute(
  new NavigationRoute(
    new NetworkFirst({
      cacheName: "mxr-pages-v1",
      networkTimeoutSeconds: 4,
      plugins: [requireConnection],
    }),
    {
      denylist: [/^\/api\//],
    },
  ),
);

registerRoute(
  ({ url }) => url.origin === self.location.origin && url.pathname.startsWith("/assets/"),
  new CacheFirst({
    cacheName: "mxr-runtime-assets-v1",
    plugins: [new ExpirationPlugin({ maxEntries: 250, purgeOnQuotaError: true })],
  }),
);

setCatchHandler(async ({ request }) => {
  if (new URL(request.url).pathname.startsWith("/api/")) {
    return Response.json(
      { error: "This item isn't saved offline. Reconnect to load it.", code: "offline_not_cached" },
      { status: 503, headers: { [OFFLINE_HEADER]: "1" } },
    );
  }
  if (request.mode === "navigate") {
    return (await matchPrecache("/index.html")) ?? Response.error();
  }
  return Response.error();
});
