/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { components } from "@/api/generated";
import { reregisterCatalog } from "@/lib/actions/catalog";
import { setServedKeys } from "@/lib/actions/definition";
import { getRegistry } from "@/lib/actions/registry";
import { useKeymapStore } from "@/state/keymapStore";

import type { KeysChange, KeysUpdate } from "./keysApi";

const updateKeys = vi.fn<(change: KeysChange, dryRun: boolean) => Promise<KeysUpdate>>();
vi.mock("./keysApi", () => ({
  updateKeys: (change: KeysChange, dryRun: boolean) => updateKeys(change, dryRun),
}));

const { KeybindingsSection, parseKeyList } = await import("./KeybindingsSection");

type Served = components["schemas"]["KeymapData"];

function served(overrides: Partial<Served> = {}): Served {
  return { actions: [], single_keys: true, remapped: [], problems: [], ...overrides } as Served;
}

function answer(update: Partial<KeysUpdate>): KeysUpdate {
  return { saved: false, file: "/c/keys.toml", problems: [], keymap: served(), ...update };
}

/**
 * Open the Archive editor. The page lists every key, so queries go by
 * label and stay inside the editor: role queries over the whole list are slow.
 */
function editArchive() {
  fireEvent.click(screen.getAllByLabelText("Change keys for Archive")[0]!);
  const input = screen.getByLabelText(/Keys for Archive/);
  const form = within(input.closest("form")!);
  return { input, save: form.getByText("Save").closest("button")!, form };
}

function renderSection() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <KeybindingsSection />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  updateKeys.mockReset();
  setServedKeys(null);
  reregisterCatalog();
  useKeymapStore.setState({ singleKeys: true, remapped: [], problems: [] });
});

describe("KeybindingsSection", () => {
  it("reads keys as a comma-separated list, sequences kept whole", () => {
    expect(parseKeyList(" x,  g   x ,, Mod+Shift+k ")).toEqual(["x", "g x", "Mod+Shift+k"]);
    expect(parseKeyList("  ")).toEqual([]);
  });

  it("checks a new key before saving it, and saves it for both clients", async () => {
    const keymap = served({
      actions: [{ id: "mail.archive", keys: ["%"] } as Served["actions"][number]],
      remapped: ["mail.archive"],
    });
    updateKeys.mockImplementation(async (_change, dryRun) => answer({ saved: !dryRun, keymap }));
    renderSection();
    const { input, save, form } = editArchive();
    expect(save).toBeDisabled();

    fireEvent.change(input, { target: { value: "%" } });
    expect(await form.findByText(/Archive will be % in the web app and the TUI/)).toBeVisible();
    expect(updateKeys).toHaveBeenCalledWith(
      { kind: "set_action", id: "mail.archive", keys: ["%"] },
      true,
    );
    expect(save).toBeEnabled();

    fireEvent.click(save);
    await waitFor(() =>
      expect(updateKeys).toHaveBeenLastCalledWith(
        { kind: "set_action", id: "mail.archive", keys: ["%"] },
        false,
      ),
    );
    await waitFor(() => expect(getRegistry().get("mail.archive")?.shortcut).toBe("%"));
    expect(useKeymapStore.getState().remapped).toEqual(["mail.archive"]);
  });

  it("names a conflict and keeps Save off", async () => {
    updateKeys.mockResolvedValue(
      answer({
        problems: ["mail.archive: web scope `list`: `s` is bound to mail.archive, mail.star"],
      }),
    );
    renderSection();
    const { input, save, form } = editArchive();
    fireEvent.change(input, { target: { value: "s" } });
    expect(await form.findByText(/is bound to mail.archive, mail.star/)).toBeVisible();
    expect(save).toBeDisabled();
    expect(updateKeys).not.toHaveBeenCalledWith(expect.anything(), false);
  });

  it("turns single-key shortcuts off through the daemon", async () => {
    updateKeys.mockResolvedValue(answer({ saved: true, keymap: served({ single_keys: false }) }));
    renderSection();
    fireEvent.click(screen.getByLabelText("Single-key shortcuts"));
    await waitFor(() =>
      expect(updateKeys).toHaveBeenCalledWith({ kind: "single_keys", on: false }, false),
    );
    await waitFor(() => expect(useKeymapStore.getState().singleKeys).toBe(false));
  });
});

describe("an action left without a key", () => {
  it("is listed so its keys can come back", async () => {
    const { applyServedKeymap } = await import("@/lib/actions/servedKeymap");
    applyServedKeymap(
      served({
        actions: [{ id: "mail.archive", keys: [] } as unknown as Served["actions"][number]],
        remapped: ["mail.archive"],
      }),
    );
    renderSection();
    expect(screen.getByText("Yours, without a key")).toBeVisible();
    fireEvent.click(screen.getAllByLabelText("Change keys for Archive")[0]!);
    expect(screen.getByText("Use the default keys")).toBeVisible();
  });
});
