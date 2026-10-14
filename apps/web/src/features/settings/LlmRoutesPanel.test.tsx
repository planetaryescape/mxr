import { render, screen, within } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { LlmRoutesPanel, type LlmFeatureStatus, type LlmKeyStatus } from "./LlmRoutesPanel";

const routed: LlmFeatureStatus = {
  feature: "relationship_summary",
  enabled: true,
  model: "claude-haiku-5-5",
  routed: true,
  cloud: true,
  routes: [
    {
      provider: "anthropic",
      model: "claude-haiku-5-5",
      state: "broken",
      reason: "401 invalid key",
      retry_at: "2026-10-14T14:32:00Z",
      next: false,
    },
    {
      provider: "openrouter",
      model: "anthropic/claude-haiku-5.5",
      state: "ok",
      next: true,
    },
  ],
};

const local: LlmFeatureStatus = {
  feature: "summarize",
  enabled: true,
  model: "gemma4",
  routed: false,
  cloud: false,
  base_url: "http://localhost:11434/v1",
  routes: [],
};

const keys: LlmKeyStatus[] = [
  { provider: "anthropic", present: true, source: "stored", env_var: "ANTHROPIC_API_KEY" },
  { provider: "openrouter", present: false, env_var: "OPENROUTER_API_KEY" },
];

describe("LlmRoutesPanel", () => {
  test("shows each route in order with its health and the one in use", () => {
    render(<LlmRoutesPanel features={[routed]} keys={keys} />);
    const row = screen.getByText("Relationship summary").closest("li");
    expect(row).not.toBeNull();
    const scoped = within(row as HTMLElement);
    expect(scoped.getByText("claude-haiku-5-5")).toBeVisible();
    expect(scoped.getByText(/Anthropic: broken: 401 invalid key/)).toBeVisible();
    expect(scoped.getByText(/OpenRouter: ok \(in use\)/)).toBeVisible();
  });

  test("labels a model that isn't routed with its endpoint and where it runs", () => {
    render(<LlmRoutesPanel features={[local]} keys={keys} />);
    expect(screen.getByText("http://localhost:11434/v1")).toBeVisible();
    expect(screen.getByText("local")).toBeVisible();
  });

  test("shows key presence without ever offering a field to enter one", () => {
    render(<LlmRoutesPanel features={[routed]} keys={keys} />);
    const list = screen.getByRole("list", { name: "Provider keys" });
    expect(within(list).getByText(/key stored/)).toBeVisible();
    expect(within(list).getByText(/no key \(set OPENROUTER_API_KEY or store one\)/)).toBeVisible();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByText(/mxr llm key set anthropic/)).toBeVisible();
  });

  test("says a disabled feature is off and renders nothing for an older daemon", () => {
    const { rerender, container } = render(
      <LlmRoutesPanel features={[{ ...local, enabled: false }]} keys={keys} />,
    );
    expect(screen.getByText("Off")).toBeVisible();
    rerender(<LlmRoutesPanel features={undefined} keys={undefined} />);
    expect(container).toBeEmptyDOMElement();
  });
});
