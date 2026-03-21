// @vitest-environment node

import { describe, expect, it } from "vitest";
import config from "./vite.config";

describe("vite desktop config", () => {
  it("uses relative asset paths for file:// renderer loads", () => {
    expect(config.base).toBe("./");
  });
});
