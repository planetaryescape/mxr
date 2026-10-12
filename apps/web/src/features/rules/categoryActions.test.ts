import { describe, expect, it } from "vitest";
import { chooseCategoryAction } from "./api";

describe("choosing category actions", () => {
  it("preserves every existing provider and treatment action", () => {
    expect(
      chooseCategoryAction("archive,treatment:reading,add-category:old,label:Receipts", "new"),
    ).toBe("archive,treatment:reading,add-category:old,label:Receipts,add-category:new");
  });
  it("replaces only the decision for the selected category", () => {
    expect(chooseCategoryAction("star,remove-category:travel,add-category:home", "travel")).toBe(
      "star,add-category:travel,add-category:home",
    );
  });
});
