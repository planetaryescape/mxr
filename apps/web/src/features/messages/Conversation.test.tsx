import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import type { Conversation } from "./api";
import { ConversationView } from "./Conversation";

const conversation: Conversation = {
  account_id: "acct",
  thread_id: "thread-1",
  subject: "Contract renewal",
  shape: "one_to_one",
  state: "your_turn",
  earlier_count: 0,
  composer: {
    label: "Reply to Samir · Contract renewal",
    reply_to_message_id: "m1",
    reply_all: false,
  },
  messages: [
    {
      message_id: "m1",
      from: { email: "samir@launchpad.example", name: "Samir Patel" },
      from_me: false,
      date: "2026-10-06T16:00:00Z",
      text: "(only quoted text)",
      only_quoted: true,
      trimmed: { quote: true, signature: false, footer: false },
      trimmed_label: "trimmed: quote",
      layout: "compact",
      paragraphs: 1,
    },
  ],
};

describe("ConversationView", () => {
  test("a quote-only message says so and offers the message as sent", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView conversation={conversation} asSent={null} onToggleAsSent={() => {}} />
      </QueryClientProvider>,
    );
    expect(screen.getByTestId("only-quoted").textContent).toBe("(only quoted text)");
    expect(screen.getByTestId("trimmed-marker").textContent).toContain("trimmed: quote");
  });
});
