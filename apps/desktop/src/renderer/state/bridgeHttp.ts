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
