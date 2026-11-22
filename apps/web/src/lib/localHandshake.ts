import { getBridgeBaseUrl, setToken } from "@/lib/tokenStorage";

interface HandshakeResponse {
  token?: string;
  source?: string;
}

let inflight: Promise<string | undefined> | undefined;

export async function tryLocalHandshake(): Promise<string | undefined> {
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const res = await fetch(`${getBridgeBaseUrl()}/api/v1/auth/local-token`, {
        method: "GET",
        headers: { accept: "application/json" },
        credentials: "same-origin",
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
      setTimeout(() => {
        inflight = undefined;
      }, 0);
    }
  })();
  return inflight;
}
