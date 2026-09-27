/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { ThreadContext, ThreadGist } from "./api";
import { ContextBlock, type GistState } from "./ContextBlock";

const FACTS: ThreadContext = {
  thread_id: "t1",
  account_id: "a1",
  counterparty: {
    email: "maya@example.com",
    display_name: "Maya Ortiz",
    messages_from_them: 2,
    messages_from_you: 1,
    your_reply_samples: 0,
    their_reply_samples: 0,
    bulk_sender: false,
  },
  commitments: [],
};

const READY: ThreadGist = {
  thread_id: "t1",
  status: "ready",
  gist: "Canary stays at 5% until the dashboard is quiet.",
  ask: {
    summary: "confirm who owns the rollout check",
    quote: { message_id: "m1", text: "Can you confirm who owns the rollout check?" },
  },
  provenance: { model: "qwen2.5", locality: "local", sources: ["this_thread"] },
  from_cache: false,
};

function gistState(overrides: Partial<GistState> = {}): GistState {
  return { reserved: true, loading: false, retry: vi.fn<() => void>(), ...overrides };
}

function renderBlock(gist: GistState, onRevealAsk = vi.fn<() => void>()) {
  render(
    <ContextBlock
      context={FACTS}
      gist={gist}
      onRevealAsk={onRevealAsk}
      onResolvePromise={vi.fn<(id: string) => void>()}
      resolving={false}
    />,
  );
  return { onRevealAsk };
}

describe("ContextBlock", () => {
  test("without a model it shows the facts and says nothing about AI", () => {
    renderBlock(gistState({ reserved: false }));
    expect(screen.getByTestId("thread-context-facts")).toHaveTextContent(
      "You and Maya: 3 emails · your first conversation",
    );
    expect(screen.queryByTestId("thread-gist")).toBeNull();
    expect(screen.getByTestId("thread-context")).not.toHaveTextContent(/\bmodel\b|\bAI\b/);
  });

  test("the gist slot keeps one fixed height while loading and when ready", () => {
    renderBlock(gistState({ loading: true }));
    const slot = screen.getByTestId("thread-gist");
    expect(slot).toHaveAttribute("data-state", "loading");
    expect(slot.className).toMatch(/(^| )h-\[9rem\] .*@xl:h-\[7\.5rem\]/);
    expect(slot).toHaveTextContent("Reading this conversation…");
  });

  test("a ready gist shows the ask, its source and a way to the quote", () => {
    const { onRevealAsk } = renderBlock(gistState({ data: READY }));
    const slot = screen.getByTestId("thread-gist");
    expect(slot).toHaveAttribute("data-state", "ready");
    expect(slot.className).toMatch(/(^| )h-\[9rem\] .*@xl:h-\[7\.5rem\]/);
    expect(screen.getByTestId("thread-ask")).toHaveTextContent(
      "Asks you confirm who owns the rollout check",
    );
    expect(screen.getByTestId("thread-gist-source")).toHaveTextContent(
      "Local model qwen2.5 · from this thread",
    );
    fireEvent.click(screen.getByRole("button", { name: "Show in message" }));
    expect(onRevealAsk).toHaveBeenCalledOnce();
  });

  test("no ask reads as nothing asked", () => {
    renderBlock(gistState({ data: { ...READY, ask: null } }));
    expect(screen.getByTestId("thread-ask")).toHaveTextContent("Nothing asked of you.");
  });

  test("a blocked model explains itself and offers no retry", () => {
    renderBlock(
      gistState({
        data: { thread_id: "t1", status: "blocked", from_cache: false, reason: "cloud" },
      }),
    );
    expect(screen.getByTestId("thread-gist")).toHaveTextContent(
      "Privacy settings keep this conversation from the configured model.",
    );
    expect(screen.queryByRole("button", { name: /Try again/ })).toBeNull();
  });
});
