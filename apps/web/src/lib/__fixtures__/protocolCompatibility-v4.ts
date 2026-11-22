export interface BridgeHealthResponseV4 {
  status?: string;
  service?: string;
  protocol_version?: number;
}

export function evaluateProtocolCompatibility(health: BridgeHealthResponseV4):
  | {
      kind: "mismatch";
      actualProtocol: number | null;
      requiredProtocol: number;
      updateSteps: string[];
      detail: string;
    }
  | undefined {
  if (health.protocol_version === 4) return undefined;

  return {
    kind: "mismatch",
    actualProtocol: typeof health.protocol_version === "number" ? health.protocol_version : null,
    requiredProtocol: 4,
    updateSteps: [
      "brew upgrade mxr",
      "rerun ./install.sh",
      "git pull && cargo install --path . --locked",
    ],
    detail: "mxr Web needs a compatible bridge before it can safely use daemon APIs.",
  };
}
