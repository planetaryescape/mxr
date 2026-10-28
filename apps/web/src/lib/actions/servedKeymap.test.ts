import { afterEach, describe, expect, it, vi } from "vitest";

import { useKeymapStore } from "@/state/keymapStore";

import { reregisterCatalog } from "./catalog";
import { setServedKeys, withServedKeys, type DefinedAction } from "./definition";
import { getRegistry } from "./registry";
import { applyServedKeymap } from "./servedKeymap";

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
