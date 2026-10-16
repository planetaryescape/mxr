import { describe, expect, test } from "vitest";

import { checkScript } from "./check-ai-marks.mjs";

describe("check-ai-marks", () => {
  test.each([
    'import { Sparkles } from "lucide-react";',
    'import { Mail, Sparkle } from "lucide-react";',
    'import { WandSparkles as Magic } from "lucide-react";',
    'import { BotMessageSquare } from "lucide-react";',
    'import {\n  Mail,\n  SparklesIcon,\n} from "lucide-react";',
    'import { type LucideIcon, Sparkles } from "lucide-react"',
  ])("%s is flagged", (source) => {
    expect(checkScript(source)).toHaveLength(1);
  });

  test("names the decision", () => {
    expect(checkScript('import { Sparkles } from "lucide-react";')[0].message).toContain("D141");
  });

  test("the sparkle emoji is flagged, with its line", () => {
    expect(checkScript("const a = 1;\nconst label = '✨ Draft';")).toEqual([
      expect.objectContaining({ line: 2 }),
    ]);
  });

  test("task icons and other packages pass", () => {
    expect(checkScript('import { PenLine, TextQuote } from "lucide-react";')).toEqual([]);
    expect(checkScript('import { Sparkles } from "somewhere-else";')).toEqual([]);
  });
});
