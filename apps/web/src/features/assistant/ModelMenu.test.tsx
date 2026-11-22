/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { AssistantAvailability, AssistantModels } from "./api";
import { ModelMenu } from "./ModelMenu";

const api = vi.hoisted(() => ({
  fetchAvailability: vi.fn<() => Promise<AssistantAvailability>>(),
  fetchModels: vi.fn<() => Promise<AssistantModels>>(),
  setModel: vi.fn<(model: string) => Promise<AssistantModels>>(),
}));
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  ...api,
}));

const choices: AssistantModels["choices"] = [
  {
    id: "gemma4:latest",
    label: "Gemma 4",
    cloud: false,
    available: true,
    sees: "this conversation and your past mail",
  },
  {
    id: "gpt-6-luna",
    label: "GPT-6 Luna",
    cloud: true,
    available: false,
    sees: "this conversation only",
    unavailable_reason: "Add an OpenAI key: mxr llm key set openai",
  },
  {
    id: "claude-haiku-5-5",
    label: "Claude Haiku 5.5",
    cloud: true,
    available: true,
    sees: "this conversation and your past mail",
  },
];

function wrap(
  children: ReactNode,
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>);
}

async function openMenu() {
  const trigger = await screen.findByRole("button", { name: /^Model: / });
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });
  return screen.findByRole("menu");
}

describe("ModelMenu", () => {
  beforeEach(() => {
    api.fetchAvailability.mockResolvedValue({
      available: true,
      model: "gemma4",
      leaves_machine: false,
    });
    api.fetchModels.mockResolvedValue({ current: "gemma4:latest", choices });
  });
  afterEach(() => vi.clearAllMocks());

  test("lists every choice with where it runs, a check on the one in use, and why one can't be picked", async () => {
    wrap(<ModelMenu />);
    const menu = await openMenu();
    const items = within(menu).getAllByRole("menuitem");
    expect(items).toHaveLength(3);
    expect(items[0]).toHaveTextContent("Gemma 4");
    expect(items[0]).toHaveTextContent("on this machine");
    expect(within(items[0]!).getByLabelText("In use")).toBeVisible();
    expect(items[2]).toHaveTextContent("cloud");
    expect(within(items[2]!).queryByLabelText("In use")).toBeNull();
    expect(items[1]).toHaveAttribute("data-disabled");
    expect(items[1]).toHaveTextContent("Add an OpenAI key: mxr llm key set openai");
  });

  test("the mark follows the choice once the daemon says the model changed", async () => {
    api.setModel.mockResolvedValue({ current: "claude-haiku-5-5", choices });
    wrap(<ModelMenu />);
    const menu = await openMenu();
    api.fetchAvailability.mockResolvedValue({
      available: true,
      model: "claude-haiku-5-5",
      leaves_machine: true,
    });
    fireEvent.click(within(menu).getByRole("menuitem", { name: /Claude Haiku 5.5/ }));
    expect(
      await screen.findByRole("button", { name: "Model: Claude Haiku 5.5. Change it" }),
    ).toHaveTextContent("Claude Haiku 5.5 · cloud");
  });

  test("a pick refreshes the Settings > LLM data, so an open form can't save the old model back", async () => {
    api.setModel.mockResolvedValue({ current: "claude-haiku-5-5", choices });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(client, "invalidateQueries");
    wrap(<ModelMenu />, client);
    const menu = await openMenu();
    fireEvent.click(within(menu).getByRole("menuitem", { name: /Claude Haiku 5.5/ }));

    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith(
        expect.objectContaining({ queryKey: ["llm-config"] }),
      ),
    );
    expect(invalidate).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ["llm-status"] }));
  });

  test("a daemon without the picker keeps the plain mark", async () => {
    api.fetchModels.mockRejectedValue(new Error("404"));
    wrap(<ModelMenu />);
    expect(await screen.findByText("gemma4 · local")).toBeVisible();
    expect(screen.queryByRole("button", { name: /^Model: / })).toBeNull();
  });
});
