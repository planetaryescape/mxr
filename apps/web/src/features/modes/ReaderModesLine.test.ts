import { describe, expect, test } from "vitest";

import { modeOfPath } from "./ReaderModesLine";

describe("the mode a reader is open in", () => {
  test("follows the route's first segment", () => {
    expect(modeOfPath("/messages/t1")).toBe("messages");
    expect(modeOfPath("/desk/t1")).toBe("messages");
    expect(modeOfPath("/todo/t1")).toBe("todo");
    expect(modeOfPath("/updates/t1")).toBe("updates");
    expect(modeOfPath("/reading/t1")).toBe("reading");
  });

  test("Now and the Inbox are no mode, so every holding mode is named", () => {
    expect(modeOfPath("/now")).toBeNull();
    expect(modeOfPath("/m/inbox/t1")).toBeNull();
    expect(modeOfPath("/search/t1")).toBeNull();
  });
});
