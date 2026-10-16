import { describe, expect, test } from "vitest";

import { labelColor } from "./LabelBadge";

describe("labelColor", () => {
  test("keeps a valid hex, expanding the short form", () => {
    expect(labelColor("Work", "#ff8800")).toBe("#ff8800");
    expect(labelColor("Work", "#f80")).toBe("#ff8800");
  });

  test("never lets anything but a hex reach a style value", () => {
    for (const hostile of [
      "red; background: url(//evil.example)",
      "url(javascript:alert(1))",
      "#12345g",
      "#1234567",
      "var(--x)",
      "",
    ]) {
      expect(labelColor("Work", hostile)).toMatch(/^#[0-9a-f]{6}$/i);
    }
    expect(labelColor("Work", null)).toMatch(/^#[0-9a-f]{6}$/i);
  });

  test("a name gives the same colour every time", () => {
    expect(labelColor("Receipts")).toBe(labelColor("Receipts"));
  });
});
