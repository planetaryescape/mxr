import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useUiPrefs } from "@/state/uiPrefsStore";

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
      has_preserved_context: false,
      only_quoted: true,
      trimmed: { quote: true, signature: false, footer: false },
      trimmed_label: "trimmed: quote",
      layout: "compact",
      paragraphs: 1,
    },
  ],
};

describe("ConversationView", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    useUiPrefs.setState(useUiPrefs.getInitialState(), true);
    document.documentElement.removeAttribute("data-scheme");
  });

  test("opens the latest letter in full while keeping older messages folded", () => {
    const longThread: Conversation = {
      ...conversation,
      messages: [
        {
          ...conversation.messages[0]!,
          text: "Older first paragraph.\n\nOlder second paragraph.",
          only_quoted: false,
          layout: "letter",
          paragraphs: 2,
        },
        {
          ...conversation.messages[0]!,
          message_id: "m2",
          text: "Hi Alex,\n\nThe main point.\n\nThe final detail.",
          only_quoted: false,
          layout: "letter",
          paragraphs: 3,
        },
      ],
    };
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView conversation={longThread} asSent={null} onToggleAsSent={() => {}} />
      </QueryClientProvider>,
    );

    const [older, latest] = screen.getAllByTestId("conversation-message");
    expect(older).toHaveTextContent("Read all 2 paragraphs");
    expect(latest).toHaveTextContent("The final detail.");
    expect(latest).not.toHaveTextContent("Read all 3 paragraphs");
  });

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
