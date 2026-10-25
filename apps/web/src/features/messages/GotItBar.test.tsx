/* @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import type { AckPlan } from "./api";
import { GotItBar } from "./GotItBar";

afterEach(() => vi.useRealTimers());

test("shows Got it's exact preview destination and sending address", () => {
  vi.useFakeTimers();
  const plan = {
    account_id: "work",
    built_from: "Your usual greeting and sign-off with Samir.",
    countdown_seconds: 5,
    dry_run: true,
    from: "alias@example.com",
    html: "<p>Got it.</p>",
    preview_token: "preview",
    reply_to_message_id: "message-1",
    subject: "Re: Contract renewal",
    text: "Got it, thanks.",
    thread_id: "thread-1",
    to: [{ name: "Samir Patel", email: "reply-to@example.com" }],
  } satisfies AckPlan;
  const onUndo = vi.fn<() => void>();
  render(<GotItBar pending={{ plan, endsAt: Date.now() + 5_000 }} onUndo={onUndo} />);

  expect(screen.getByText("Samir Patel <reply-to@example.com>")).toBeInTheDocument();
  expect(screen.getByText("alias@example.com")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /Undo/ }));
  expect(onUndo).toHaveBeenCalledOnce();
});
