/* @vitest-environment node */

import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

import { describe, expect, test } from "vitest";

import { MXR_VERSION, olderThan, withoutExpiredMoves, type DefinedAction } from "./definition";

const catchUp: DefinedAction = {
  id: "todo.catchup",
  label: "Catch up",
  group: "Navigate",
  scopes: ["todo"],
  keys: ["2"],
  moved: [
    { key: "C", contexts: ["todo"], until: "0.6.197" },
    { key: "c", contexts: ["todo"], until: "0.6.200" },
  ],
};

describe("moved keys expire (D171)", () => {
  test("a move is ignored from its until release on", () => {
    expect(withoutExpiredMoves([catchUp], "0.6.196")[0]).toBe(catchUp);
    expect(withoutExpiredMoves([catchUp], "0.6.197")[0]?.moved).toEqual([
      { key: "c", contexts: ["todo"], until: "0.6.200" },
    ]);
    expect(withoutExpiredMoves([catchUp], "0.6.200")[0]?.moved).toEqual([]);
  });

  test("versions compare by number, as crates/keymap compares them", () => {
    expect(olderThan("0.6.99", "0.6.197")).toBe(true);
    expect(olderThan("0.6.197", "0.6.197")).toBe(false);
    expect(olderThan("0.7.0", "0.6.197")).toBe(false);
    expect(olderThan("0.6", "0.6.0")).toBe(true);
  });

  test("this build expires them at the workspace version, as the TUI does", () => {
    const manifest = readFileSync(
      fileURLToPath(new URL("../../../../../Cargo.toml", import.meta.url)),
      "utf8",
    );
    expect(manifest).toContain(`[workspace.package]\nversion = "${MXR_VERSION}"`);
  });
});
