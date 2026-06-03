export const EXPECTED_IPC_PROTOCOL_VERSION = 4;

export interface BridgeHealthResponse {
  status?: string;
  service?: string;
  protocol_version?: number;
}

export interface ProtocolMismatch {
  kind: "mismatch";
  actualProtocol: number | null;
  requiredProtocol: number;
  updateSteps: string[];
  detail: string;
}

export function buildProtocolUpdateSteps(): string[] {
  return ["brew upgrade mxr", "rerun ./install.sh", "git pull && cargo install --path . --locked"];
}

export function evaluateProtocolCompatibility(
  health: BridgeHealthResponse,
): ProtocolMismatch | undefined {
  if (health.protocol_version === EXPECTED_IPC_PROTOCOL_VERSION) return undefined;

  return {
    kind: "mismatch",
    actualProtocol: typeof health.protocol_version === "number" ? health.protocol_version : null,
    requiredProtocol: EXPECTED_IPC_PROTOCOL_VERSION,
    updateSteps: buildProtocolUpdateSteps(),
    detail: "mxr Web needs a compatible bridge before it can safely use daemon APIs.",
  };
}
