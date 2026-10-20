import fs from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";

import { deliver } from "../e2e/helpers/spool";

let root: string | undefined;
afterEach(() => {
  vi.restoreAllMocks();
  if (root) fs.rmSync(root, { recursive: true, force: true });
});

it("ingests sent history before its reply even when both are delivered in one millisecond", () => {
  root = fs.mkdtempSync(join(tmpdir(), "mxr-spool-order-"));
  fs.mkdirSync(join(root, ".playwright"));
  fs.mkdirSync(join(root, "spool"));
  fs.writeFileSync(join(root, ".playwright/state.json"), JSON.stringify({ runtimeDir: root }));
  vi.spyOn(process, "cwd").mockReturnValue(root);
  vi.spyOn(Date, "now").mockReturnValue(123);
  vi.spyOn(Math, "random").mockReturnValueOnce(0.9).mockReturnValueOnce(0.1);
  const first = deliver({
    from: "alex@example.com",
    subject: "sent",
    thread: "fixture",
    labels: ["SENT"],
  });
  const second = deliver({ from: "friend@example.com", subject: "reply", thread: "fixture" });
  expect(first.localeCompare(second)).toBeLessThan(0);
  expect(fs.readFileSync(join(root, "spool", first + ".msg"), "utf8")).toContain("Thread: fixture");
  expect(fs.readFileSync(join(root, "spool", first + ".msg"), "utf8")).toContain("Labels: SENT");
  expect(fs.readFileSync(join(root, "spool", second + ".msg"), "utf8")).toContain("Subject: reply");
});
