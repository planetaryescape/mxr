import { useEffect, useRef } from "react";

import type { ActionScope } from "@/lib/actions/types";

export type ScopeCommand = () => void;
export type ScopeController = Partial<Record<string, ScopeCommand>>;

const controllers = new Map<ActionScope, ScopeController>();

export function setController(scope: ActionScope, controller: ScopeController | null): void {
  if (controller) controllers.set(scope, controller);
  else controllers.delete(scope);
}

export function getController(scope: ActionScope): ScopeController | undefined {
  return controllers.get(scope);
}

export function hasCommand(scope: ActionScope, command: string): boolean {
  return typeof controllers.get(scope)?.[command] === "function";
}

export function runCommand(scope: ActionScope, command: string): boolean {
  const run = controllers.get(scope)?.[command];
  if (typeof run !== "function") return false;
  run();
  return true;
}

export function useScopeController(scope: ActionScope, controller: ScopeController): void {
  const ref = useRef(controller);
  ref.current = controller;
  useEffect(() => {
    const proxy = new Proxy({} as ScopeController, {
      get: (_target, command: string) => ref.current[command],
      has: (_target, command: string) => command in ref.current,
    });
    setController(scope, proxy);
    return () => {
      if (controllers.get(scope) === proxy) controllers.delete(scope);
    };
  }, [scope]);
}
