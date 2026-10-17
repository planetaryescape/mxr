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

export async function openApp(page: Pick<Page, "goto" | "waitForLoadState">, path = "/") {
  const state = readE2EState();
  const url = `${path}#token=${encodeURIComponent(state.token)}`;
  // WebKit rejects goto when the router navigates again before the load event:
  // the app's own redirect (/rules to /onboarding), or the previous page's late
  // navigation (/todo to /todo/<id>), which can swallow this goto. Chromium
  // resolves both. Retry once after the page settles; a redirect that interrupts
  // again is the app landing where it meant to, and the specs assert where.
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      await page.goto(url);
      return;
    } catch (error) {
      const interrupted =
        error instanceof Error &&
        /interrupted by another navigation|Frame load interrupted/.test(error.message);
      if (!interrupted) throw error;
      await page.waitForLoadState("load");
    }
  }
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
