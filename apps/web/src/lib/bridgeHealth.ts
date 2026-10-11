import type { BridgeHealthResponse } from "@/lib/protocolCompatibility";
import { getBridgeBaseUrl } from "@/lib/tokenStorage";

function isObjectRecord(
  value: unknown,
): value is { status?: unknown; service?: unknown; protocol_version?: unknown } {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export async function fetchBridgeHealth(signal?: AbortSignal): Promise<BridgeHealthResponse> {
  const response = await fetch(`${getBridgeBaseUrl()}/api/v1/health`, { signal });
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  const payload: unknown = await response.json();
  if (!isObjectRecord(payload)) throw new Error("Invalid bridge health response");

  const protocolVersion = payload.protocol_version;
  const status = payload.status;
  const service = payload.service;
  if (
    protocolVersion !== undefined &&
    (typeof protocolVersion !== "number" || !Number.isInteger(protocolVersion))
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
