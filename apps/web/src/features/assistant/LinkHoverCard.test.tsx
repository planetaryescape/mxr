/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  createEvent,
  fireEvent,
  render as renderPlain,
  screen,
  type RenderOptions,
} from "@testing-library/react";
import type { ReactElement } from "react";
import { createJSONStorage } from "zustand/middleware";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AssistantSplit } from "./AssistantSplit";
import { useAssistantUi } from "./assistantStore";
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

// The narrow-screen drawer, with the real sidebar swapped for a reply.
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
/** The bridge, by path: membership for the glyphs, previews and a person's page for the cards. */
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
const fetched = (part: string) => apiFetch.mock.calls.filter(([path]) => path.includes(part));

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

/** Time passes for the hover card's timers and for the query that follows. */
const wait = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

/** A pointer event with a type, as the browser would send it. */
function pointer(el: HTMLElement, kind: "Over" | "Out" | "Down" | "Up", pointerType: string) {
  const event = createEvent[`pointer${kind}` as const](el);
  Object.defineProperty(event, "pointerType", { value: pointerType });
  fireEvent(el, event);
}

/** React turns a `pointerover` from outside the element into `onPointerEnter`. */
function pointerEnter(chip: HTMLElement, pointerType: string) {
  const event = createEvent.pointerOver(chip);
  Object.defineProperty(event, "pointerType", { value: pointerType });
  fireEvent(chip, event);
}

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

  test("keyboard focus opens it after the delay, with what the chip knows first and the rest filled in", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    await wait(0);
    expect(fetched("/threads/previews")).toHaveLength(0);

    act(() => chip.focus());
    await wait(399);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
    expect(fetched("/threads/previews")).toHaveLength(0);

    // The card opens before any data: the chip's own subject, no spinner.
    await act(() => vi.advanceTimersByTimeAsync(1));
    const card = screen.getByTestId("link-hover-card");
    expect(card).toHaveTextContent("Q1 report");
    expect(card.querySelector("[role=status]")).toBeNull();

    await wait(10);
    expect(fetched("/threads/previews")).toHaveLength(1);
    expect(card).toHaveTextContent("Invoice 4471 overdue");
    expect(card).toHaveTextContent("Dana Ruiz <dana@example.com>");
    expect(card).toHaveTextContent("3 messages");
    expect(card).toHaveTextContent("Please confirm payment by Friday");
    expect(card).toHaveTextContent("To do");
    expect(card).toHaveTextContent("Attachment");
    expect(card).toHaveTextContent("1 unread");
  });

  test("Escape closes it", async () => {
    reply();
    act(() => screen.getByRole("link", { name: "Q1 report" }).focus());
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    await wait(10);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
  });

  test("the pointer opens it, and touch never does", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointerEnter(chip, "touch");
    await wait(1000);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
    expect(fetched("/threads/previews")).toHaveLength(0);

    pointerEnter(chip, "mouse");
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
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

  test("a person's card shows their page and says when it is your turn", async () => {
    reply("Ask [Diana Lee](mxr:person/person:d@x.com).");
    const chip = screen.getByRole("link", { name: /Diana Lee/ });
    act(() => chip.focus());
    await wait(450);
    const card = screen.getByTestId("link-hover-card");
    expect(card).toHaveTextContent("Diana Lee");
    // A profile read: the bridge is told to leave the conversation out.
    expect(fetched("/people/page")).toHaveLength(1);
    expect(fetched("/people/page")[0]?.[0]).toContain("conversation=false");
    await wait(10);
    expect(card).toHaveTextContent("close · usually 47m");
    expect(card).toHaveTextContent("You've written 48 times since 2023.");
    expect(card).toHaveTextContent("Your turn");
    expect(fetched("/threads/previews")).toHaveLength(0);
  });

  /** What a browser does on a click while hovering: press, focus, release, click. */
  function clickWithFocus(chip: HTMLElement) {
    fireEvent.pointerDown(chip);
    act(() => chip.focus());
    fireEvent.pointerUp(chip);
    fireEvent.click(chip);
  }

  /** Time passes and no card appears and nothing is fetched. */
  const never = async () => {
    await wait(1500);
    return [screen.queryByTestId("link-hover-card"), fetched("/threads/previews")] as const;
  };

  test("a click during the open delay, then leaving, never opens the card", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointerEnter(chip, "mouse");
    await wait(200);
    clickWithFocus(chip);
    expect(navigate).toHaveBeenCalledWith({ href: "/m/archive/t-1234567" });
    await wait(20);
    fireEvent.pointerOut(chip);
    const [card, calls] = await never();
    expect(card).toBeNull();
    expect(calls).toHaveLength(0);

    // Coming back is a fresh hover.
    act(() => chip.blur());
    pointerEnter(chip, "mouse");
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("a click during the open delay, then blurring, never opens the card", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointerEnter(chip, "mouse");
    await wait(200);
    clickWithFocus(chip);
    await wait(20);
    act(() => chip.blur());
    const [card, calls] = await never();
    expect(card).toBeNull();
    expect(calls).toHaveLength(0);

    // The pointer leaves and comes back: a fresh session.
    fireEvent.pointerOut(chip);
    pointerEnter(chip, "mouse");
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("a brief re-hover after a click does not let the first timer through", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointerEnter(chip, "mouse");
    await wait(200);
    clickWithFocus(chip);
    await wait(20);
    fireEvent.pointerOut(chip);
    await wait(10);
    pointerEnter(chip, "mouse");
    await wait(20);
    fireEvent.pointerOut(chip); // focus stays on the link
    const [card, calls] = await never();
    expect(card).toBeNull();
    expect(calls).toHaveLength(0);
  });

  test("pressing, leaving, releasing outside and tabbing away and back still opens on focus", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointerEnter(chip, "mouse");
    fireEvent.pointerDown(chip);
    act(() => chip.focus());
    fireEvent.pointerOut(chip); // released elsewhere: no pointerup or click here
    await wait(1000);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();

    act(() => chip.blur());
    act(() => chip.focus());
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("Enter on a focused link does not open the card after it", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    act(() => chip.focus());
    await wait(100);
    fireEvent.keyDown(chip, { key: "Enter" });
    fireEvent.click(chip);
    const [card, calls] = await never();
    expect(card).toBeNull();
    expect(calls).toHaveLength(0);
  });

  test("an ordinary hover opens it, and one timer serves hover plus focus", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointerEnter(chip, "mouse");
    await wait(300);
    act(() => chip.focus());
    await wait(150);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
    await wait(300);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  /** Hover until the card is open, then put the pointer on the card. */
  async function openAndEnterCard(chip: HTMLElement) {
    pointer(chip, "Over", "mouse");
    await wait(450);
    const card = screen.getByTestId("link-hover-card");
    pointer(chip, "Out", "mouse");
    pointer(card, "Over", "mouse");
    return card;
  }

  test("Esc after moving onto the card and back keeps it closed until a fresh hover", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    const card = await openAndEnterCard(chip);
    pointer(card, "Out", "mouse");
    pointer(chip, "Over", "mouse");
    await wait(100);
    fireEvent.keyDown(document.body, { key: "Escape" });
    await wait(1500);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();

    pointer(chip, "Out", "mouse");
    pointer(chip, "Over", "mouse");
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("after a touch tap in the order down, up, leave, click a mouse hover still opens it", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointer(chip, "Down", "touch");
    pointer(chip, "Up", "touch");
    pointer(chip, "Out", "touch");
    fireEvent.click(chip);
    expect(navigate).toHaveBeenCalledTimes(1);
    await wait(1000);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();

    pointer(chip, "Over", "mouse");
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("after a touch tap in the order down, up, leave, click keyboard focus still opens it", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointer(chip, "Down", "touch");
    pointer(chip, "Up", "touch");
    pointer(chip, "Out", "touch");
    fireEvent.click(chip);
    act(() => chip.blur());
    act(() => chip.focus());
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("pressing inside the card, leaving it and releasing outside closes it after the grace", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    const card = await openAndEnterCard(chip);
    pointer(card, "Down", "mouse");
    pointer(card, "Out", "mouse"); // released outside: no pointerup or click here
    await wait(150);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
    await wait(100);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
  });

  test("leaving the link closes the card after the grace, and moving onto the card keeps it", async () => {
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointer(chip, "Over", "mouse");
    await wait(450);
    pointer(chip, "Out", "mouse");
    await wait(100);
    pointer(screen.getByTestId("link-hover-card"), "Over", "mouse");
    await wait(500);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();
  });

  test("unmounting while a timer is pending updates nothing", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
    reply();
    const chip = screen.getByRole("link", { name: "Q1 report" });
    pointer(chip, "Over", "mouse");
    await wait(100);
    cleanup();
    await wait(1000);
    expect(errors).not.toHaveBeenCalled();
    errors.mockRestore();
  });

  test("in the drawer, Escape closes the card and leaves the assistant open", async () => {
    // jsdom here has no localStorage for the store's persistence to write to.
    useAssistantUi.persist.setOptions({
      storage: createJSONStorage(() => ({
        getItem: () => null,
        setItem: () => undefined,
        removeItem: () => undefined,
      })),
    });
    useAssistantUi.setState({ open: true });
    render(<AssistantSplit>{null}</AssistantSplit>);
    // The sidebar is lazy: let its import settle.
    await wait(100);
    const chip = screen.getByRole("link", { name: "Q1 report" });
    act(() => chip.focus());
    await wait(450);
    expect(screen.getByTestId("link-hover-card")).toBeInTheDocument();

    fireEvent.keyDown(chip, { key: "Escape" });
    await wait(10);
    expect(screen.queryByTestId("link-hover-card")).toBeNull();
    expect(useAssistantUi.getState().open).toBe(true);

    // With no card open, Escape still closes the drawer.
    fireEvent.keyDown(chip, { key: "Escape" });
    expect(useAssistantUi.getState().open).toBe(false);
  });
});
