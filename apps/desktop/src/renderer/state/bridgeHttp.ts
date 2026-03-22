export async function fetchJson<T>(
  baseUrl: string,
  authToken: string,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Content-Type", "application/json");
  headers.set("x-mxr-bridge-token", authToken);

  const response = await fetch(new URL(path, `${baseUrl}/`), {
    ...init,
    headers,
  });

  if (!response.ok) {
    throw new Error(`Request failed: ${response.status}`);
  }

  return (await response.json()) as T;
}
