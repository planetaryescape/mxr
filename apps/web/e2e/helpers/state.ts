import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export interface E2EState {
  bridgeUrl: string;
  controlUrl: string;
  token: string;
  runtimeDir: string;
  writtenAt: string;
}

export function readE2EState(): E2EState {
  return JSON.parse(readFileSync(resolve(".playwright/state.json"), "utf8")) as E2EState;
}

export async function openApp(page: { goto: (url: string) => Promise<unknown> }, path = "/") {
  const state = readE2EState();
  await page.goto(`${path}#token=${encodeURIComponent(state.token)}`);
}

export async function stopDaemon() {
  const state = readE2EState();
  await controlFetch(`${state.controlUrl}/daemon/stop`);
}

export async function restartDaemon() {
  const state = readE2EState();
  await controlFetch(`${state.controlUrl}/daemon/restart`);
}

async function controlFetch(url: string) {
  const response = await fetch(url, { method: "POST" });
  if (!response.ok) throw new Error(`${url} ${response.status}: ${await response.text()}`);
}
