/*
 * Same-machine bridge-token handshake.
 *
 * The bridge exposes an unauthenticated endpoint `/api/v1/auth/local-token`
 * that returns its own token to callers whose TCP peer is a loopback IP.
 * On a same-machine deploy (the default for `mxr web`) this lets the SPA
 * authenticate without making the user paste anything.
 *
 * If the operator has disabled the feature (`[bridge].auto_local_token =
 * false`) or the peer is not loopback (remote-host mode), the endpoint
 * returns 404 and we fall back to the paste-token UI.
 */

import { getBridgeBaseUrl, setToken } from "@/lib/tokenStorage";

interface HandshakeResponse {
  token?: string;
  source?: string;
}

let inflight: Promise<string | undefined> | undefined;

/**
 * Attempt the same-machine handshake. Returns the token on success and
 * `undefined` on any failure (network, 404, malformed body). Caches the
 * in-flight promise so concurrent callers share one request.
 */
export async function tryLocalHandshake(): Promise<string | undefined> {
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const res = await fetch(`${getBridgeBaseUrl()}/api/v1/auth/local-token`, {
        method: "GET",
        headers: { accept: "application/json" },
        credentials: "omit",
      });
      if (!res.ok) return undefined;
      const body = (await res.json()) as HandshakeResponse;
      if (typeof body.token === "string" && body.token.length > 0) {
        setToken(body.token);
        return body.token;
      }
      return undefined;
    } catch {
      return undefined;
    } finally {
      // Reset so a future 401 (e.g. after token rotation) can re-handshake.
      setTimeout(() => {
        inflight = undefined;
      }, 0);
    }
  })();
  return inflight;
}
