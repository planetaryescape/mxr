import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { ThreadResponse } from "@/features/mailbox/types";
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

  test("each message's line says when it arrived, in full", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView conversation={conversation} asSent={null} onToggleAsSent={() => {}} />
      </QueryClientProvider>,
    );
    const line = screen.getByTestId("conversation-message").querySelector("p");
    // The fixture is from 2026-10-06, so the line carries the date and the time.
    expect(line?.textContent).toMatch(/^Samir · .*6.*Oct.*\d{1,2}:\d{2}/);
    expect(line?.textContent).not.toMatch(/ago|just now/);
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

  test("a provider preview is labeled incomplete and offers original recovery", () => {
    const onToggleAsSent = vi.fn<() => void>();
    const preview: Conversation = {
      ...conversation,
      messages: [
        {
          ...conversation.messages[0]!,
          text: "Can you make Thursday?",
          only_quoted: false,
          source: "provider_snippet",
        },
      ],
    };
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView conversation={preview} asSent={null} onToggleAsSent={onToggleAsSent} />
      </QueryClientProvider>,
    );

    expect(screen.getByTestId("provider-preview").textContent).toContain("Provider preview only");
    fireEvent.click(screen.getByRole("button", { name: "Check the original" }));
    expect(onToggleAsSent).toHaveBeenCalledOnce();
  });

  test("an empty provider preview still names the missing message", () => {
    const preview: Conversation = {
      ...conversation,
      messages: [
        {
          ...conversation.messages[0]!,
          text: "",
          only_quoted: false,
          source: "provider_snippet",
        },
      ],
    };
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView conversation={preview} asSent={null} onToggleAsSent={() => {}} />
      </QueryClientProvider>,
    );
    expect(screen.getByTestId("provider-preview").textContent).toContain(
      "No provider preview is available",
    );
  });

  test("a long inline exchange stays complete even when the cached ask is one answer", () => {
    const mixed: Conversation = {
      ...conversation,
      messages: [
        {
          ...conversation.messages[0]!,
          text: "> Can you ship Thursday?\n\nYes, but only if QA passes.\n\n> Can we skip the migration?\n\nNo, do not skip it; the revised date is Friday.",
          has_preserved_context: true,
          only_quoted: false,
          layout: "letter",
          paragraphs: 4,
          ask_quote: "No, do not skip it",
        },
      ],
    };
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ConversationView conversation={mixed} asSent={null} onToggleAsSent={() => {}} />
      </QueryClientProvider>,
    );

    const body = screen.getByTestId("conversation-message");
    expect(body.textContent).toContain("Can you ship Thursday?");
    expect(body.textContent).toContain("only if QA passes");
    expect(body.textContent).toContain("Can we skip the migration?");
    expect(body.textContent).toContain("revised date is Friday");
    expect(screen.queryByRole("button", { name: /Read all/ })).toBeNull();
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

  test("Check original follows global and sender HTML appearance preferences", async () => {
    const threadResponse: ThreadResponse = {
      thread: {
        account_id: "acct",
        id: "thread-1",
        latest_date: "2026-10-06T16:00:00Z",
        message_count: 1,
        participants: [],
        snippet: "Original message",
        subject: "Contract renewal",
        unread_count: 0,
      },
      messages: [],
      bodies: [{ message_id: "m1", text_html: "<p>Original message</p>" }],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve(
          new Response(JSON.stringify(threadResponse), {
            headers: { "Content-Type": "application/json" },
          }),
        ),
      ),
    );
    class TestMediaQueryList extends EventTarget implements MediaQueryList {
      readonly media = "(prefers-color-scheme: light)";
      onchange: ((this: MediaQueryList, event: MediaQueryListEvent) => void) | null = null;

      constructor(public matches: boolean) {
        super();
      }

      addListener(_listener: ((this: MediaQueryList, event: MediaQueryListEvent) => void) | null) {}
      removeListener(
        _listener: ((this: MediaQueryList, event: MediaQueryListEvent) => void) | null,
      ) {}
    }
    const media = new TestMediaQueryList(false);
    vi.stubGlobal(
      "matchMedia",
      vi.fn(() => media),
    );
    document.documentElement.dataset.scheme = "dark";
    useUiPrefs.setState({
      theme: "system",
      emailHtmlTheme: "auto",
      emailHtmlSenderOverrides: [],
    });

    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <ConversationView conversation={conversation} asSent="m1" onToggleAsSent={() => {}} />
      </QueryClientProvider>,
    );
    const frame = await screen.findByTitle("HTML message body");
    const expectCanvas = async (scheme: string) =>
      waitFor(() => expect(frame).toHaveStyle({ colorScheme: scheme }));

    await expectCanvas("dark");
    media.matches = true;
    act(() => media.dispatchEvent(new Event("change")));
    await expectCanvas("light");

    act(() => useUiPrefs.getState().setEmailHtmlTheme("original"));
    await expectCanvas("light");
    act(() => useUiPrefs.getState().setEmailHtmlTheme("dark"));
    await expectCanvas("dark");

    act(() => {
      useUiPrefs.getState().setEmailHtmlTheme("original");
      useUiPrefs.getState().setEmailHtmlThemeForSender("samir@launchpad.example", "dark");
    });
    await expectCanvas("dark");
    act(() =>
      useUiPrefs.getState().setEmailHtmlThemeForSender("samir@launchpad.example", "original"),
    );
    await expectCanvas("light");
    act(() => {
      useUiPrefs.getState().setEmailHtmlTheme("dark");
      useUiPrefs.getState().forgetEmailHtmlThemeForSender("samir@launchpad.example");
    });
    await expectCanvas("dark");
  });
});
