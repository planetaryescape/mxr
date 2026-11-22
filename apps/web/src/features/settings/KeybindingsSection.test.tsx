/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { components } from "@/api/generated";
import { reregisterCatalog } from "@/lib/actions/catalog";
import { setServedKeys } from "@/lib/actions/definition";
import { useKeymapStore } from "@/state/keymapStore";

import type { KeysChange, KeysUpdate } from "./keysApi";

const updateKeys = vi.fn<(change: KeysChange, dryRun: boolean) => Promise<KeysUpdate>>();
vi.mock("./keysApi", () => ({
  updateKeys: (change: KeysChange, dryRun: boolean) => updateKeys(change, dryRun),
}));

const { KeybindingsSection } = await import("./KeybindingsSection");

type Served = components["schemas"]["KeymapData"];

function served(overrides: Partial<Served> = {}): Served {
  return { actions: [], single_keys: true, remapped: [], problems: [], ...overrides } as Served;
}

function answer(update: Partial<KeysUpdate>): KeysUpdate {
  return { saved: false, file: "/c/keys.toml", problems: [], keymap: served(), ...update };
}

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
});
