import { afterEach, describe, expect, it, vi } from "vitest";

import { useKeymapStore } from "@/state/keymapStore";

import { reregisterCatalog } from "./catalog";
import { setServedKeys, withServedKeys, type DefinedAction } from "./definition";
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

  it("keys this build can't register leave the defaults in place", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    applyServedKeymap({
      actions: [{ id: "mail.archive", keys: ["s"] }],
      remapped: ["mail.archive"],
    });
    expect(warn).toHaveBeenCalled();
    expect(boundTo("list", "e")).toBe("mail.archive");
    expect(boundTo("list", "s")).toBe("mail.star");
  });

  it("an action not remapped keeps every key it was defined with", () => {
    const defined = new Map<string, DefinedAction>([
      [
        "a",
        { id: "a", label: "A", group: "Mail", keys: ["x"], mac_keys: ["Ctrl+x"], scopes: ["list"] },
      ],
    ]);
    const result = withServedKeys(defined, { actions: [{ id: "a", keys: ["y"] }] });
    expect(result.get("a")).toEqual(defined.get("a"));
  });

  it("a moved key's notice goes where the person now uses that key", () => {
    const defined = new Map<string, DefinedAction>([
      [
        "old",
        {
          id: "old",
          label: "Old",
          group: "Mail",
          keys: ["O"],
          scopes: ["list", "todo"],
          moved: [{ key: "z", contexts: ["list", "todo"], until: "9.9.9" }],
        },
      ],
      ["new", { id: "new", label: "New", group: "Mail", keys: ["n"], scopes: ["list"] }],
    ]);
    const result = withServedKeys(defined, {
      actions: [{ id: "new", keys: ["z"] }],
      remapped: ["new"],
    });
    expect(result.get("new")?.keys).toEqual(["z"]);
    expect(result.get("old")?.moved).toEqual([{ key: "z", contexts: ["todo"], until: "9.9.9" }]);
  });
});

describe("the keymap is not on the first-paint path", () => {
  it("an answer is cached and the next boot applies it before any request", () => {
    apiFetch.mockResolvedValue({
      keymap: { actions: [{ id: "mail.archive", keys: ["%"] }], remapped: ["mail.archive"] },
    });

    return loadServedKeymap().then(() => {
      expect(boundTo("list", "%")).toBe("mail.archive");
      expect(window.localStorage.getItem(CACHE_KEY)).toContain("mail.archive");
      expect(readCachedKeymap()).toMatchObject({
        actions: [{ id: "mail.archive", keys: ["%"] }],
        remapped: ["mail.archive"],
      });
    });
  });

  it("a later answer updates the registry and bumps the store's version", async () => {
    // The app has painted with the defaults: the request lands afterwards.
    expect(boundTo("list", "e")).toBe("mail.archive");
    const before = useKeymapStore.getState().version;
    apiFetch.mockResolvedValue({
      keymap: {
        actions: [{ id: "mail.archive", keys: ["%"] }],
        remapped: ["mail.archive"],
        single_keys: false,
      },
    });

    await loadServedKeymap();

    expect(boundTo("list", "%")).toBe("mail.archive");
    expect(boundTo("list", "e")).toBeUndefined();
    expect(useKeymapStore.getState().version).toBe(before + 1);
    expect(useKeymapStore.getState().singleKeys).toBe(false);
  });

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

  it("ignores a cached record far larger than a served keymap", () => {
    const raw = JSON.stringify({ keymap: { actions: [], padding: "x".repeat(1_000_001) } });
    expect(parseCachedKeymap(raw)).toBeNull();
  });
});
