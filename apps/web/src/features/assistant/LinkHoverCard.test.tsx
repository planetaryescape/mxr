/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render as renderPlain,
  screen,
  type RenderOptions,
} from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { Transcript } from "./Transcript";

const navigate = vi.fn<(options: { href: string }) => Promise<void>>();
vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}));
vi.mock("@/features/mail-actions/mailQueryInvalidation", () => ({
  invalidateMailQueries: () => Promise.resolve(),
}));

const apiFetch = vi.hoisted(() => vi.fn<(path: string, init?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch,
}));

vi.mock("@/hooks/useMediaQuery", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/hooks/useMediaQuery")>()),
  useMediaQuery: () => true,
}));
vi.mock("./AssistantSidebar", async () => {
  const { Transcript: Reply } = await import("./Transcript");
  return {
    AssistantSidebar: () => (
      <Reply
        conversationId="c1"
        running={false}
        items={[
          {
            kind: "assistant",
            seq: 1,
            text: "See [Q1 report](mxr:thread/t-1234567).",
            created_at: 0,
          },
        ]}
      />
    ),
  };
});

const preview = {
  thread_id: "t-1234567",
  subject: "Invoice 4471 overdue",
  sender_name: "Dana Ruiz",
  sender_email: "dana@example.com",
  latest_at: 1_760_000_000,
  message_count: 3,
  unread_count: 1,
  snippet: "Please confirm payment by Friday",
  has_attachments: true,
};
const page = {
  row: { title: "Diana Lee", your_turn: true },
  header_line: "close · usually 47m",
  relationship_line: "You've written 48 times since 2023.",
};

let previewBody: unknown;
function answer(path: string) {
  if (path.endsWith("/modes/membership")) {
    return Promise.resolve({
      kind: "ModeMembership",
      threads: [
        {
          thread_id: "t-1234567",
          in_inbox: false,
          modes: [{ mode: "todo", name: "To do" }],
          account_id: "a",
          subject: "x",
        },
      ],
    });
  }
  if (path.endsWith("/threads/previews")) {
    return Promise.resolve({ kind: "ThreadPreviews", previews: [previewBody] });
  }
  if (path.includes("/people/page")) return Promise.resolve({ kind: "PersonPage", page });
  return Promise.reject(new Error(`unexpected ${path}`));
}

function render(ui: ReactElement, options?: RenderOptions) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return renderPlain(ui, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
    ...options,
  });
}

function reply(text = "See [Q1 report](mxr:thread/t-1234567).") {
  render(
    <Transcript
      conversationId="c1"
      running={false}
      items={[{ kind: "assistant", seq: 1, text, created_at: 0 }]}
    />,
  );
}

const wait = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

describe("the hover card on an email or person chip", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal(
      "ResizeObserver",
      class ResizeObserver {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
    previewBody = preview;
    apiFetch.mockReset();
    apiFetch.mockImplementation((path) => answer(path));
    navigate.mockClear();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  test("a hostile subject and snippet stay text", async () => {
    previewBody = {
      ...preview,
      subject: '<img src=x onerror="alert(1)"> **Urgent**',
      snippet: "<script>alert(1)</script>",
    };
    reply();
    act(() => screen.getByRole("link", { name: "Q1 report" }).focus());
    await wait(450);
    await wait(10);
    const card = screen.getByTestId("link-hover-card");
    expect(card).toHaveTextContent('<img src=x onerror="alert(1)"> **Urgent**');
    expect(card).toHaveTextContent("<script>alert(1)</script>");
    expect(card.querySelector("img, script, strong")).toBeNull();
  });

  test("the chip is still a link that navigates, and a click closes the card", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    expect(chip.tagName).toBe("A");
    act(() => chip.focus());
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
    fireEvent.click(chip);
    expect(navigate).toHaveBeenCalledWith({ href: "/m/archive/t-1234567" });
    await wait(10);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
  });
});
