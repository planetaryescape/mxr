/*
 * Scope controllers. A mounted view (mail list, reader, sidebar, screener)
 * registers the commands it can perform; registry actions bound to that
 * scope call them by name. This keeps every key in one table while the
 * behaviour stays with the view that owns the state.
 */

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

/** Run a command on a scope's controller. Returns false when unavailable. */
export function runCommand(scope: ActionScope, command: string): boolean {
  const run = controllers.get(scope)?.[command];
  if (typeof run !== "function") return false;
  run();
  return true;
}

/**
 * Register `controller` for `scope` while mounted. The latest controller
 * object is read at call time, so callers can pass a fresh object every
 * render without re-registering.
 */
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
