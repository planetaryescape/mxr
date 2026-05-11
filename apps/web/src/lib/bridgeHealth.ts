import type { BridgeHealthResponse } from "@/lib/protocolCompatibility";
import { getBridgeBaseUrl } from "@/lib/tokenStorage";

export async function fetchBridgeHealth(signal?: AbortSignal): Promise<BridgeHealthResponse> {
  const response = await fetch(`${getBridgeBaseUrl()}/api/v1/health`, { signal });
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return (await response.json()) as BridgeHealthResponse;
}
