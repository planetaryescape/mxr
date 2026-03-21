export interface MxrStatusSnapshot {
  protocol_version: number;
  daemon_version: string | null;
  daemon_build_id?: string | null;
}

export interface BridgeReadyState {
  kind: "ready";
  baseUrl: string;
  authToken: string;
  binaryPath: string;
  usingBundled: boolean;
  daemonVersion: string | null;
  protocolVersion: number;
}

export interface BridgeMismatchState {
  kind: "mismatch";
  binaryPath: string;
  usingBundled: boolean;
  daemonVersion: string | null;
  actualProtocol: number | null;
  requiredProtocol: number;
  updateSteps: string[];
  detail: string;
}

export interface BridgeErrorState {
  kind: "error";
  binaryPath: string;
  usingBundled: boolean;
  title: string;
  detail: string;
}

export interface BridgeIdleState {
  kind: "idle";
}

export type BridgeState =
  | BridgeReadyState
  | BridgeMismatchState
  | BridgeErrorState
  | BridgeIdleState;

export interface DesktopApi {
  getBridgeState(): Promise<BridgeState>;
  retryBridge(): Promise<BridgeState>;
  useBundledMxr(): Promise<BridgeState>;
  setExternalBinaryPath(path: string): Promise<BridgeState>;
}
