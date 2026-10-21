import { createJSONStorage } from "zustand/middleware";
import { beforeEach, describe, expect, test } from "vitest";

import { useAssistantUi } from "./assistantStore";
import { privateStorage } from "@/test/privateStorage";

const ui = () => useAssistantUi.getState();

describe("how the panel was opened", () => {
  const storage = privateStorage();
  beforeEach(() => {
    // Node 26's inert localStorage leaves the store with no storage of its own.
    useAssistantUi.persist.setOptions({ storage: createJSONStorage(() => storage) });
    storage.clear();
    useAssistantUi.setState({ open: false, via: null });
  });

  test("a pointer opens it with a slide to play; a key opens it still", () => {
    ui().toggle("pointer");
    expect(ui()).toMatchObject({ open: true, via: "pointer" });
    ui().toggle("pointer");
    expect(ui()).toMatchObject({ open: false, via: null });
    ui().toggle("key");
    expect(ui()).toMatchObject({ open: true, via: null });
  });

  test("closing mid-slide leaves nothing for the next open to replay", () => {
    ui().toggle("pointer");
    ui().setOpen(false);
    expect(ui().via).toBeNull();
    ui().setOpen(true, "key");
    expect(ui().via).toBeNull();
  });

  test("opening a panel that is already open never starts a slide", () => {
    ui().toggle("key");
    ui().setOpen(true, "pointer");
    expect(ui().via).toBeNull();
  });

  test("the slide is spent once it has played", () => {
    ui().toggle("pointer");
    ui().settle();
    expect(ui()).toMatchObject({ open: true, via: null });
  });

  test("a reload never persists a slide", () => {
    ui().toggle("pointer");
    const saved = JSON.parse(storage.getItem("mxr:assistant") ?? "{}") as {
      state: Record<string, unknown>;
    };
    expect(saved.state).toEqual({ open: true, conversationId: null });
  });
});
