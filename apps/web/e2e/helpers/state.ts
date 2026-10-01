import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, type Page } from "@playwright/test";

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

export const bridgeAuth = () => ({ authorization: `Bearer ${readE2EState().token}` });

/** Call the bridge directly (GET, or POST with `body`) and return its JSON. */
export async function bridge<T>(page: Page, path: string, body?: unknown): Promise<T> {
  const url = `${readE2EState().bridgeUrl}${path}`;
  const response = body
    ? await page.request.post(url, { headers: bridgeAuth(), data: body })
    : await page.request.get(url, { headers: bridgeAuth() });
  expect(response.ok(), `${path}: ${response.status()}`).toBeTruthy();
  return (await response.json()) as T;
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
