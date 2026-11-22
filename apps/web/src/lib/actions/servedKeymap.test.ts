import { afterEach, describe, expect, it, vi } from "vitest";

import { useKeymapStore } from "@/state/keymapStore";

import { reregisterCatalog } from "./catalog";
import { setServedKeys } from "./definition";
import { getRegistry } from "./registry";
import {
  applyServedKeymap,
  loadServedKeymap,
  parseCachedKeymap,
  readCachedKeymap,
} from "./servedKeymap";

const apiFetch = vi.hoisted(() => vi.fn<(path: string) => Promise<unknown>>());
vi.mock("@/api/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/client")>();
  return { ...actual, apiFetch };
});

const CACHE_KEY = `mxr.servedKeymap.v1:${window.location.origin}`;

function boundTo(scope: string, chord: string): string | undefined {
  return getRegistry()
    .bindings()
    .find((binding) => binding.scope === scope && binding.chord === chord && !binding.moved)?.action
    .id;
}

afterEach(() => {
  setServedKeys(null);
  reregisterCatalog();
  useKeymapStore.setState({ singleKeys: true, remapped: [], problems: [] });
  window.localStorage.removeItem(CACHE_KEY);
  apiFetch.mockReset();
  vi.restoreAllMocks();
});

describe("the person's keys from the daemon", () => {
  it("a keys.toml remap moves the action's key in the registry", () => {
    expect(boundTo("list", "e")).toBe("mail.archive");
    applyServedKeymap({
      actions: [{ id: "mail.archive", keys: ["%"] }],
      remapped: ["mail.archive"],
      single_keys: false,
    });
    expect(boundTo("list", "%")).toBe("mail.archive");
    expect(boundTo("reader", "%")).toBe("mail.archive");
    expect(boundTo("list", "e")).toBeUndefined();
    expect(getRegistry().get("mail.archive")?.shortcut).toBe("%");
    expect(useKeymapStore.getState()).toMatchObject({
      singleKeys: false,
      remapped: ["mail.archive"],
    });
  });
});

describe("the keymap is not on the first-paint path", () => {
  it("a failed request keeps the cached keys working", async () => {
    window.localStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ keymap: { actions: [{ id: "mail.archive", keys: ["%"] }] } }),
    );
    expect(readCachedKeymap()).not.toBeNull();
    apiFetch.mockRejectedValue(new Error("offline"));
    useKeymapStore.setState({ singleKeys: true, remapped: [], problems: [] });

    await loadServedKeymap();

    expect(boundTo("list", "e")).toBe("mail.archive");
    expect(readCachedKeymap()).not.toBeNull();
  });

  it("drops a cached record that isn't the shape the daemon serves", () => {
    const bad = [
      "not json",
      "[]",
      JSON.stringify({ keymap: { actions: "nope" } }),
      JSON.stringify({ keymap: { actions: [{ id: 7 }] } }),
      JSON.stringify({ keymap: { actions: [{ id: "__proto__", keys: ["x"] }] } }),
      JSON.stringify({ keymap: { actions: [{ id: "ok", keys: ["x", 3] }] } }),
      JSON.stringify({ keymap: { actions: [], single_keys: "no" } }),
      JSON.stringify({ keymap: { actions: [], remapped: [{}] } }),
      JSON.stringify({ keymap: { actions: [{ id: "ok", keys: [42] }] } }),
    ];
    for (const raw of bad) expect(parseCachedKeymap(raw)).toBeNull();

    expect(
      parseCachedKeymap(
        JSON.stringify({
          keymap: {
            actions: [{ id: "mail.archive", keys: ["%"], mac_keys: ["Cmd+%"] }],
            single_keys: false,
            remapped: ["mail.archive"],
            problems: ["ignored: x"],
          },
        }),
      ),
    ).toEqual({
      actions: [{ id: "mail.archive", keys: ["%"], mac_keys: ["Cmd+%"] }],
      single_keys: false,
      remapped: ["mail.archive"],
      problems: ["ignored: x"],
    });
  });
});
