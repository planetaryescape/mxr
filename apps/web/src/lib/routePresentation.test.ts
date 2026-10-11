import { describe, expect, it } from "vitest";

import { routeTitleForPath } from "./routePresentation";

describe("routeTitleForPath", () => {
  it("uses generic page titles for message routes", () => {
    expect(routeTitleForPath("/messages/a-private-subject-id")).toBe("Messages");
    expect(routeTitleForPath("/m/inbox/a-private-subject-id")).toBe("Mail");
  });

  it("names the active settings section", () => {
    expect(routeTitleForPath("/settings/about")).toBe("About");
    expect(routeTitleForPath("/settings/unknown")).toBe("Settings");
  });

  it("never treats inherited object keys as route titles", () => {
    expect(routeTitleForPath("/constructor")).toBe("mxr");
    expect(routeTitleForPath("/settings/toString")).toBe("Settings");
  });
});
