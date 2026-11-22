import { existsSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { readE2EState } from "./state";

function spoolDir(): string {
  return join(readE2EState().runtimeDir, "spool");
}

let deliverySequence = 0;

export interface SpoolMessage {
  from: string;
  to?: string;
  subject: string;
  body?: string;
  thread?: string;
  labels?: string[];
  date?: string;
}

export function deliver(message: SpoolMessage): string {
  const name = `${Date.now()}-${String(deliverySequence++).padStart(8, "0")}-${Math.random().toString(36).slice(2, 8)}`;
  const headers = [
    `From: ${message.from}`,
    message.to ? `To: ${message.to}` : null,
    `Subject: ${message.subject}`,
    message.thread ? `Thread: ${message.thread}` : null,
    message.labels ? `Labels: ${message.labels.join(", ")}` : null,
    message.date ? `Date: ${message.date}` : null,
  ].filter(Boolean);
  writeFileSync(
    join(spoolDir(), `${name}.msg`),
    `${headers.join("\n")}\n\n${message.body ?? ""}\n`,
  );
  return name;
}

export function failSyncs(error: string): void {
  writeFileSync(join(spoolDir(), "fail"), `${error}\n`);
}

export function restoreSyncs(): void {
  rmSync(join(spoolDir(), "fail"), { force: true });
}

export function rejectMutationFor(providerId: string): () => void {
  const path = join(spoolDir(), "mutation-reject");
  if (existsSync(path)) throw new Error("fake-provider mutation rejection is already configured");
  writeFileSync(path, `${providerId}\n`);
  return () => rmSync(path, { force: true });
}
