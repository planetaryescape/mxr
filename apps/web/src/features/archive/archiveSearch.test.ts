import { describe, expect, test } from "vitest";

import { filterFromSearch, parseArchiveSearch, searchFromFilter } from "./archiveSearch";

describe("Archive's URL", () => {
  test("a filter survives the trip through the URL", () => {
    const filter = {
      year: 2025,
      issuer: "Acme",
      kinds: ["receipt" as const, "order" as const],
      checked: false,
      has_pdf: true,
      group_id: "trip-1",
      min_amount_minor: 500,
      max_amount_minor: 9_900,
    };
    expect(filterFromSearch(parseArchiveSearch({ ...searchFromFilter(filter) }))).toEqual(filter);
  });

  test("junk in a pasted link is dropped, not passed to the daemon", () => {
    const search = parseArchiveSearch({
      year: "twenty",
      kinds: ["receipt", "spaceship"],
      checked: "yes",
      section: "everything",
      min: 1.5,
      record: "",
    });
    expect(search).toEqual(expect.objectContaining({ kinds: ["receipt"] }));
    expect(filterFromSearch(search)).toEqual({ kinds: ["receipt"] });
    expect(search.section).toBeUndefined();
    expect(search.record).toBeUndefined();
  });

  test("clearing the filter clears every filter param", () => {
    const cleared = searchFromFilter({});
    expect(Object.keys(cleared).toSorted()).toEqual([
      "checked",
      "group",
      "issuer",
      "kinds",
      "max",
      "min",
      "pdf",
      "year",
    ]);
    expect(Object.values(cleared).every((value) => value === undefined)).toBe(true);
  });
});
