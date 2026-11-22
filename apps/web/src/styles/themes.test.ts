/* @vitest-environment node */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, test } from "vitest";

import { renderThemesCss, THEMES } from "./themes";

describe.each(THEMES)("$id as generated", () => {});

test("themes.generated.css is current (run `npm run gen:themes`)", () => {
  const committed = readFileSync(
    fileURLToPath(new URL("./themes.generated.css", import.meta.url)),
    "utf8",
  );
  expect(committed).toBe(renderThemesCss());
});
