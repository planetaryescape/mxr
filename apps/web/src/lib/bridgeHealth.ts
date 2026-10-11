import type { BridgeHealthResponse } from "@/lib/protocolCompatibility";
import { getBridgeBaseUrl } from "@/lib/tokenStorage";

export async function fetchBridgeHealth(signal?: AbortSignal): Promise<BridgeHealthResponse> {
  const response = await fetch(`${getBridgeBaseUrl()}/api/v1/health`, { signal });
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  const payload: unknown = await response.json();
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) {
    throw new Error("Invalid bridge health response");
  }

  const fields = payload as Record<string, unknown>;
  const protocolVersion = fields.protocol_version;
  const status = fields.status;
  const service = fields.service;
  if (
    protocolVersion !== undefined &&
    (!Number.isInteger(protocolVersion) || typeof protocolVersion !== "number")
  ) {
    throw new Error("Invalid bridge protocol version");
  }
  if (status !== undefined && typeof status !== "string") {
    throw new Error("Invalid bridge health status");
  }
  if (service !== undefined && typeof service !== "string") {
    throw new Error("Invalid bridge health service");
  }

  return {
    status,
    service,
    protocol_version: protocolVersion,
  };
}
