import { fireEvent, render, screen } from "@testing-library/react";
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { describe, expect, test } from "vitest";

import type { components } from "@/api/generated";
import { DraftProvenanceLine } from "./DraftProvenanceLine";

type Provenance = components["schemas"]["DraftProvenanceData"];
type Source = NonNullable<Provenance["voice_examples"]>[number];

const source = (message_id: string): Source => ({
  message_id,
  thread_id: `thread-${message_id}`,
  date: "2026-10-01T12:00:00Z",
  from_me: true,
  person: "sam@example.com",
  person_name: "Sam Example",
});

function provenance(voice_examples: Source[]): Provenance {
  return {
    model: "gemma4",
    locality: "local",
    history_used: voice_examples.length > 0,
    voice_examples,
    conversation: [],
  };
}

async function renderProvenance(value: Provenance) {
  const rootRoute = createRootRoute({
    component: () => <DraftProvenanceLine provenance={value} />,
  });
  const router = createRouter({
    routeTree: rootRoute,
    history: createMemoryHistory({ initialEntries: ["/"] }),
  });
  await router.load();
  render(<RouterProvider router={router} />);
}

describe("DraftProvenanceLine", () => {
  test("shows the exact examples and opens each source through its message route", async () => {
    await renderProvenance(provenance([source("m1"), source("m2")]));

    expect(screen.getByTestId("draft-voice-example-count")).toHaveTextContent("Voice examples: 2");
    fireEvent.click(screen.getByRole("button", { name: "Sources (2)" }));
    expect(screen.getAllByRole("link", { name: /Your email to Sam/ })).toHaveLength(2);
    expect(screen.getAllByRole("link", { name: /Your email to Sam/ })[0]).toHaveAttribute(
      "href",
      "/m/sent/thread-m1?message=m1",
    );
  });

  test("states honestly when no voice examples were returned", async () => {
    await renderProvenance(provenance([]));
    expect(screen.getByTestId("draft-voice-example-count")).toHaveTextContent("Voice examples: 0");
    expect(screen.queryByRole("button", { name: /Sources/ })).not.toBeInTheDocument();
  });
});
