import type { BridgeMismatchState, MxrStatusSnapshot } from "../shared/types.js";

export function buildUpdateSteps(): string[] {
  return [
    "Homebrew: brew upgrade mxr",
    "Release install: rerun ./install.sh",
    "Source install: git pull && cargo install --path crates/daemon --locked",
  ];
}

export function parseStatusOutput(stdout: string): MxrStatusSnapshot {
  const parsed = JSON.parse(stdout) as Partial<MxrStatusSnapshot>;
  if (typeof parsed.protocol_version !== "number") {
    throw new Error("mxr status output is missing protocol_version");
  }
  return {
    protocol_version: parsed.protocol_version,
    daemon_version: parsed.daemon_version ?? null,
    daemon_build_id: parsed.daemon_build_id ?? null,
  };
}

export function evaluateCompatibility(input: {
  expectedProtocol: number;
  actual: MxrStatusSnapshot | null;
  binaryPath: string;
  usingBundled: boolean;
  detail?: string;
}): BridgeMismatchState | null {
  if (input.actual && input.actual.protocol_version === input.expectedProtocol) {
    return null;
  }

  return {
    kind: "mismatch",
    binaryPath: input.binaryPath,
    usingBundled: input.usingBundled,
    daemonVersion: input.actual?.daemon_version ?? null,
    actualProtocol: input.actual?.protocol_version ?? null,
    requiredProtocol: input.expectedProtocol,
    updateSteps: buildUpdateSteps(),
    detail: input.detail ?? "mxr Desktop needs a compatible version of mxr before it can connect.",
  };
}

export function assertBridgeMailboxContract(payload: unknown): void {
  if (
    !payload ||
    typeof payload !== "object" ||
    !("mailbox" in payload) ||
    !("sidebar" in payload) ||
    !("shell" in payload)
  ) {
    throw new Error("mxr web bridge returned a legacy /mailbox payload");
  }

  const parsed = payload as {
    mailbox?: { groups?: unknown[] };
    sidebar?: { sections?: unknown[] };
  };

  if (!Array.isArray(parsed.mailbox?.groups)) {
    throw new Error("mxr web bridge /mailbox response is missing mailbox.groups");
  }

  if (!Array.isArray(parsed.sidebar?.sections)) {
    throw new Error("mxr web bridge /mailbox response is missing sidebar.sections");
  }
}
