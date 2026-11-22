import { act, renderHook } from "@testing-library/react";
import { createRef, type MutableRefObject } from "react";
import { afterEach, describe, expect, it } from "vitest";

import type { ComposeDraftState } from "./composeDraft";
import { useComposeAutofocus } from "./useComposeAutofocus";

const reply = {
  current: { frontmatter: { to: "sam@example.com" } },
} as unknown as MutableRefObject<ComposeDraftState | null>;

const nextFrame = () =>
  act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));

function mountEditor(): HTMLElement {
  const surface = document.createElement("div");
  surface.setAttribute("data-compose-surface", "");
  const body = document.createElement("div");
  body.className = "cm-content";
  body.tabIndex = -1;
  surface.append(body);
  document.body.append(surface);
  return body;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("useComposeAutofocus", () => {
  it("focuses a reply's body once its lazily loaded editor mounts", async () => {
    renderHook(() => useComposeAutofocus("/drafts/r.md", reply, createRef()));
    await nextFrame();
    expect(document.activeElement).toBe(document.body);

    const body = mountEditor();
    await act(async () => {});

    expect(document.activeElement).toBe(body);
  });

  it("leaves focus in a field the person moved to while the editor loaded", async () => {
    const field = document.createElement("input");
    document.body.append(field);
    renderHook(() => useComposeAutofocus("/drafts/r.md", reply, createRef()));
    await nextFrame();
    field.focus();

    mountEditor();
    await act(async () => {});

    expect(document.activeElement).toBe(field);
  });
});
