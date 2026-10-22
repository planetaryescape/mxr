import { afterEach, describe, expect, test } from "vitest";

import { focusActivePane } from "./focusPane";

afterEach(() => {
  document.body.innerHTML = "";
});

describe("focusActivePane", () => {
  test("focuses the pane the keyboard was driving", () => {
    document.body.innerHTML = `
      <section data-mode-pane tabindex="-1"></section>
      <div data-active-pane="true" tabindex="-1"></div>`;

    focusActivePane();

    expect(document.activeElement).toBe(document.querySelector("[data-active-pane]"));
  });

  test("falls back to the mode's main view when no pane is marked", () => {
    document.body.innerHTML = `<section data-mode-pane tabindex="-1"></section>`;

    focusActivePane();

    expect(document.activeElement).toBe(document.querySelector("[data-mode-pane]"));
  });
});
