import { afterEach, describe, expect, test } from "vitest";

import { focusActivePane, refocusPaneAfter } from "./focusPane";

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

describe("refocusPaneAfter", () => {
  test("hands focus to the mode's view when closing the reader dropped it to the page", async () => {
    document.body.innerHTML = `
      <section data-mode-pane tabindex="-1"></section>
      <article tabindex="-1"></article>`;
    const reader = document.querySelector<HTMLElement>("article")!;
    reader.focus();

    // The reader closes: its pane leaves the page with focus inside.
    await refocusPaneAfter(Promise.resolve().then(() => reader.remove()));

    expect(document.activeElement).toBe(document.querySelector("[data-mode-pane]"));
  });

  test("leaves focus alone when something already holds it", async () => {
    document.body.innerHTML = `
      <section data-mode-pane tabindex="-1"></section>
      <input />`;
    const field = document.querySelector("input")!;
    field.focus();

    await refocusPaneAfter(Promise.resolve());

    expect(document.activeElement).toBe(field);
  });
});
