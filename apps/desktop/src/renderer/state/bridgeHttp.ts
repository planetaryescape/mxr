export async function fetchJson<T>(
  baseUrl: string,
  authToken: string,
  path: string,
  init?: RequestInit & { requestLabel?: string },
): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Content-Type", "application/json");
  headers.set("x-mxr-bridge-token", authToken);

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
    const msg = `${prefix}${init?.method ?? "GET"} ${path} → ${response.status}${detail ? ` — ${detail}` : ""}`;
    console.error("[bridge:http]", msg);
    throw new Error(msg);
  }

  return (await response.json()) as T;
}
