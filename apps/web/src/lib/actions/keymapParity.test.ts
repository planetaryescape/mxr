/* @vitest-environment node */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, test } from "vitest";

import "./catalog";
import {
  differenceFor,
  KEYMAP_DIFFERENCES,
  keyRows,
  SAME_ACTION,
  sameAction,
  type KeyRow,
  type TuiKeymap,
} from "./keymapParity";
import { getRegistry } from "./registry";

// Written by the TUI's keymap test (UPDATE_KEYMAP=1), from its dispatcher.
const TUI_KEYMAP = resolve(__dirname, "../../../../../docs/reference/tui-keymap.json");
const tui = JSON.parse(readFileSync(TUI_KEYMAP, "utf8")) as TuiKeymap;
const rows = keyRows(tui);

function describeRow(row: KeyRow): string {
  return `${row.context} \`${row.key}\`: web ${row.web ?? "-"}, TUI ${row.tui ?? "-"}`;
}

describe("web and TUI keys", () => {
  test("a key does the same thing in both clients unless the difference is listed", () => {
    if (process.env.PRINT_KEYMAP === "1") {
      console.info(
        rows.map((row) => `${sameAction(row) ? "=" : "≠"} ${describeRow(row)}`).join("\n"),
      );
    }
    const unexplained = rows.filter((row) => !sameAction(row) && !differenceFor(row));
    expect(unexplained.map(describeRow)).toEqual([]);
  });

  test("every listed difference is still a difference", () => {
    const explained = new Set(
      rows.filter((row) => !sameAction(row)).map((row) => `${differenceFor(row)?.why}|${row.key}`),
    );
    const stale = KEYMAP_DIFFERENCES.flatMap((entry) =>
      entry.keys
        .filter((key) => !explained.has(`${entry.why}|${key}`))
        .map((key) => `${entry.context} \`${key}\` (${entry.why})`),
    );
    expect(stale).toEqual([]);
  });

  test("every pairing names real actions and is used", () => {
    const registry = getRegistry();
    const unknown = Object.keys(SAME_ACTION).filter((id) => !registry.get(id));
    expect(unknown).toEqual([]);
    const used = new Set(rows.filter(sameAction).map((row) => row.web));
    expect(Object.keys(SAME_ACTION).filter((id) => !used.has(id))).toEqual([]);
  });
});
