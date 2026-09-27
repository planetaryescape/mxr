/* @vitest-environment jsdom */

import { afterEach, describe, expect, test } from "vitest";

import { holdTypeAhead } from "./typeAhead";

afterEach(() => {
  document.body.innerHTML = "";
});

const press = (key: string) =>
  document.body.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
  );

describe("holdTypeAhead", () => {
  test("keys typed before the field focuses are typed into it", () => {
    let pageSaw = "";
    const onPage = (event: KeyboardEvent) => (pageSaw += event.key);
    window.addEventListener("keydown", onPage);
    holdTypeAhead();
    press("s");
    press("e");

    const field = document.createElement("input");
    let inputEvents = 0;
    field.addEventListener("input", () => (inputEvents += 1));
    document.body.append(field);
    field.focus();

    expect(field.value).toBe("se");
    expect(inputEvents).toBe(1);
    // Held keys never reached the page's shortcuts (e would archive).
    expect(pageSaw).toBe("");
    window.removeEventListener("keydown", onPage);
  });

  test("stops holding once a field has focus", () => {
    holdTypeAhead();
    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    press("x");
    expect(field.value).toBe("");
    // A later key on the page is the page's again.
    let pageSaw = "";
    const onPage = (event: KeyboardEvent) => (pageSaw += event.key);
    window.addEventListener("keydown", onPage);
    press("j");
    expect(pageSaw).toBe("j");
    window.removeEventListener("keydown", onPage);
  });
});
