import type { components } from "@/api/generated";
import { getBridgeBaseUrl } from "./tokenStorage";

export type WorkspaceOwner = components["schemas"]["ArchiveWorkspaceData"];
export interface WorkspaceBinding { target: string; workspace: WorkspaceOwner }
export class WorkspaceConflictError extends Error {
  readonly code = "workspace_conflict";
  constructor() { super("Selected workspace changed. Your private edits and pending outcomes remain retained; return to the original workspace to recover."); }
}
let selected: WorkspaceBinding | undefined;
let generation = 0;
const listeners = new Set<() => void>();
export function sameOwner(a: WorkspaceOwner, b: WorkspaceOwner): boolean {
  return a.workspace_id === b.workspace_id && a.recovery_generation === b.recovery_generation;
}
export function assertTarget(target: string): void {
  if (target !== getBridgeBaseUrl()) throw new WorkspaceConflictError();
}
export function assertWorkspaceBinding(binding: WorkspaceBinding): void {
  assertTarget(binding.target);
  if (selected?.target === binding.target && !sameOwner(selected.workspace, binding.workspace)) throw new WorkspaceConflictError();
}
export function observeWorkspace(binding: WorkspaceBinding): void {
  const changed = selected != null && (selected.target !== binding.target || !sameOwner(selected.workspace, binding.workspace));
  selected = binding;
  if (changed) { generation += 1; for (const listener of listeners) listener(); }
}
export function workspaceGeneration(): number { return generation; }
export function selectedWorkspace(): WorkspaceBinding | undefined { return selected; }
export function onWorkspaceChanged(listener: () => void): () => void {
  listeners.add(listener); return () => listeners.delete(listener);
}
export function parseWorkspace(value: unknown): WorkspaceOwner | undefined {
  if (typeof value !== "object" || value === null || !("workspace_id" in value) || !("recovery_generation" in value) || typeof value.workspace_id !== "string" || typeof value.recovery_generation !== "string") return undefined;
  return { workspace_id: value.workspace_id, recovery_generation: value.recovery_generation };
}

export function resetWorkspaceOwnerForTest(): void {
  selected = undefined; generation = 0; listeners.clear();
}
