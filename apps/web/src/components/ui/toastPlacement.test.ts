import { describe, expect, it } from "vitest";

import { cornerAvoiding } from "./toastPlacement";

const view = { width: 1440, height: 900, top: 60, bottom: 40 };

describe("cornerAvoiding", () => {
  it("keeps the corner when no row is lit", () => {
    expect(cornerAvoiding("bottom-right", null, view)).toBe("bottom-right");
  });

  it("keeps the corner when the lit row is elsewhere", () => {
    const row = { left: 300, right: 700, top: 400, bottom: 452 };
    expect(cornerAvoiding("bottom-right", row, view)).toBe("bottom-right");
  });

  it("takes the other corner when the lit row is under the toast", () => {
    const row = { left: 900, right: 1424, top: 800, bottom: 852 };
    expect(cornerAvoiding("bottom-right", row, view)).toBe("top-right");
  });

  it("stays put when both corners are covered", () => {
    const column = { left: 900, right: 1424, top: 0, bottom: 900 };
    expect(cornerAvoiding("bottom-right", column, view)).toBe("bottom-right");
  });

  it("flips from the top corner too", () => {
    const row = { left: 900, right: 1424, top: 70, bottom: 122 };
    expect(cornerAvoiding("top-right", row, view)).toBe("bottom-right");
  });
});
