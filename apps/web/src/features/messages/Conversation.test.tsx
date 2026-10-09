import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
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

  test("expands the remaining authored text and collapses without losing focus", () => {
    const text = "Opening paragraph.\n\nSecond authored paragraph.\n\nFinal authored paragraph.";
    const letterConversation: Conversation = {
      ...conversation,
      messages: [
        {
          ...conversation.messages[0]!,
          text,
          only_quoted: false,
          trimmed: { quote: false, signature: false, footer: false },
          trimmed_label: undefined,
          layout: "letter",
          paragraphs: 3,
        },
      ],
    };

    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView
          conversation={letterConversation}
          asSent={null}
          onToggleAsSent={() => {}}
        />
      </QueryClientProvider>,
    );

    const readAll = screen.getByRole("button", { name: "Read all 3 paragraphs" });
    const bodyId = readAll.getAttribute("aria-controls");
    expect(readAll.getAttribute("aria-expanded")).toBe("false");
    if (!bodyId) throw new Error("The disclosure button must identify its message body");
    const body = document.getElementById(bodyId);
    expect(body?.textContent).toBe("Opening paragraph.");
    readAll.focus();

    fireEvent.click(readAll);
    const showLess = screen.getByRole("button", { name: "Show less" });
    expect(showLess.getAttribute("aria-expanded")).toBe("true");
    expect(showLess.getAttribute("aria-controls")).toBe(bodyId);
    expect(body?.textContent).toBe(
      "Opening paragraph.Second authored paragraph.Final authored paragraph.",
    );
    expect(document.activeElement).toBe(showLess);

    fireEvent.click(showLess);
    const readAllAgain = screen.getByRole("button", { name: "Read all 3 paragraphs" });
    expect(readAllAgain.getAttribute("aria-expanded")).toBe("false");
    expect(readAllAgain.getAttribute("aria-controls")).toBe(bodyId);
    expect(body?.textContent).toBe("Opening paragraph.");
    expect(document.activeElement).toBe(readAllAgain);
  });
});
