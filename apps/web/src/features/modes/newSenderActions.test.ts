import { afterEach, describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { getRegistry } from "@/lib/actions/registry";
import { setController } from "@/lib/keys/controllers";

import type { ScreenerQuestion } from "./membership";
import { newSenderCommands } from "./NewSenderChips";

const question: ScreenerQuestion = {
  account_id: "acct",
  sender_email: "uma@example.com",
  question: "New sender. Keep in Messages?",
  choices: [],
};

afterEach(() => {
  setController("now", null);
  setController("messages", null);
});

describe("keys 1 to 4 on a lit new sender", () => {
  test("only say where the retired tab went until a new sender is lit", () => {
    for (const scope of ["now", "messages"] as const) {
      setController(scope, { down: () => {}, up: () => {} });
      const binding = getRegistry().resolve("1", [scope, "global"]);
      expect(binding?.action.id).toBe("nav.tab-mail");
      expect(binding?.moved).toBe(true);
    }
  });

  test("answer the question while a new sender is lit, on both pages", () => {
    for (const scope of ["now", "messages"] as const) {
      setController(scope, newSenderCommands(question, "Uma"));
      expect(getRegistry().resolve("1", [scope, "global"])?.action.id).toBe(`${scope}.sender-1`);
      expect(getRegistry().resolve("4", [scope, "global"])?.action.id).toBe(`${scope}.sender-4`);
    }
  });
});
