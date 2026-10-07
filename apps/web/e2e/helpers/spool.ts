/*
 * The fake provider's spool (crates/provider-fake/src/spool.rs): a message
 * dropped here arrives on the next sync, and a `fail` file fails every
 * sync until it is removed.
 */

import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { readE2EState } from "./state";

function spoolDir(): string {
  return join(readE2EState().runtimeDir, "spool");
}

export interface SpoolMessage {
  from: string;
  to?: string;
  subject: string;
  body?: string;
  /** Provider label ids; the default is INBOX and UNREAD. */
  labels?: string[];
}

/** Drop a message that arrives on the next sync. Returns its file name. */
export function deliver(message: SpoolMessage): string {
  const name = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const headers = [
    `From: ${message.from}`,
    message.to ? `To: ${message.to}` : null,
    `Subject: ${message.subject}`,
    message.labels ? `Labels: ${message.labels.join(", ")}` : null,
  ].filter(Boolean);
  writeFileSync(
    join(spoolDir(), `${name}.msg`),
    `${headers.join("\n")}\n\n${message.body ?? ""}\n`,
  );
  return name;
}

/** Make every sync fail with `error` ("rate_limit 120" is a rate limit). */
export function failSyncs(error: string): void {
  writeFileSync(join(spoolDir(), "fail"), `${error}\n`);
}

export function restoreSyncs(): void {
  rmSync(join(spoolDir(), "fail"), { force: true });
}
