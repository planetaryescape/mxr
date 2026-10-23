/* @vitest-environment jsdom */

import { expect, test } from "vitest";

import { halfPageRows, listMotions } from "./listMotions";

/** A mode's cursor over `rows` rows, clamped the way every mode's `move` is. */
function cursor(rows: number, start: number) {
  const state = { index: start };
  const move = (delta: number) => {
    state.index = Math.min(rows - 1, Math.max(0, state.index + delta));
  };
  return { state, motions: listMotions(move) };
}

test("g g and G reach the first and last rows from anywhere", () => {
  const { state, motions } = cursor(40, 17);
  motions.bottom();
  expect(state.index).toBe(39);
  motions.top();
  expect(state.index).toBe(0);
});

test("Ctrl-d and Ctrl-u move half a window of rows and stop at the ends", () => {
  window.innerHeight = 880;
  expect(halfPageRows()).toBe(10);
  const { state, motions } = cursor(25, 0);
  motions.pageDown();
  expect(state.index).toBe(10);
  motions.pageDown();
  motions.pageDown();
  expect(state.index).toBe(24);
  motions.pageUp();
  expect(state.index).toBe(14);
});

test("a tiny window still moves a row", () => {
  window.innerHeight = 20;
  expect(halfPageRows()).toBe(1);
});
