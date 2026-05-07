let nextBridgeRequestId = 0;

function bridgeRequestId() {
  nextBridgeRequestId += 1;
  return String(nextBridgeRequestId);
}

export async function fetchJson<T>(
  baseUrl: string,
  authToken: string,
  path: string,
  init?: RequestInit & { requestLabel?: string },
): Promise<T> {
  const requestId = bridgeRequestId();
  const headers = new Headers(init?.headers);
  headers.set("Content-Type", "application/json");
  // v0.5+ — preferred path. Generated SDKs from the OpenAPI spec
  // emit this same header. v0.4.x daemons ignore it.
  headers.set("Authorization", `Bearer ${authToken}`);
  // v0.4.x compat. The bridge accepts both for the v0.5 cycle and
  // will drop this header in v0.6.
  headers.set("x-mxr-bridge-token", authToken);
  headers.set("x-mxr-request-id", requestId);

  const response = await fetch(new URL(path, `${baseUrl}/`), {
    ...init,
    headers,
    signal: init?.signal,
  });

  if (!response.ok) {
    let detail = "";
    try {
      detail = await response.text();
    } catch {
      /* ignore */
    }
    const prefix = init?.requestLabel ? `[${init.requestLabel}] ` : "";
    const msg = `${prefix}[req:${requestId}] ${init?.method ?? "GET"} ${path} → ${response.status}${detail ? ` — ${detail}` : ""}`;
    console.error("[bridge:http]", {
      requestId,
      requestLabel: init?.requestLabel ?? null,
      method: init?.method ?? "GET",
      path,
      status: response.status,
      detail: detail || null,
    });
    throw new Error(msg);
  }

  return (await response.json()) as T;
}
