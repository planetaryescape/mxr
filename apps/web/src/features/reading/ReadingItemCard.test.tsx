import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { ReadingItem } from "./api";
import { ReadingItemCard, type CardHandlers } from "./ReadingItemCard";

function item(extra: Partial<ReadingItem> = {}): ReadingItem {
  return {
    item_key: "m1:0",
    account_id: "a",
    message_id: "m1",
    thread_id: "t1",
    kind: "issue",
    shape: "single",
    title: "A long read",
    source: "Long Reads Weekly",
    sender_email: "essays@longreads.example",
    words: 900,
    minutes: 4,
    arrived_at: "2026-10-06T08:00:00Z",
    why: "Here because: you subscribed, and it has an unsubscribe link (rule). Fades Wed 21 Oct unless you keep it.",
    fades: "Fades Wed 21 Oct unless you keep it.",
    on_later: false,
    progress: 0,
    opened: false,
    finished: false,
    article_cached: false,
    ...extra,
  };
}

function handlers(): CardHandlers {
  return {
    onFocus: vi.fn<(key: string) => void>(),
    onRead: vi.fn<(key: string) => void>(),
    onLater: vi.fn<(key: string) => void>(),
    onLetGo: vi.fn<(item: ReadingItem) => void>(),
    onUnsubscribe: vi.fn<(item: ReadingItem) => void>(),
    onToggleLinks: vi.fn<(key: string) => void>(),
  };
}

function renderCard(focused: boolean, extra: Partial<ReadingItem> = {}) {
  return render(
    <ReadingItemCard
      item={item(extra)}
      index={0}
      linkIndex={new Map()}
      focusedKey={focused ? "m1:0" : null}
      expanded={false}
      fading={false}
      handlers={handlers()}
    />,
  );
}

describe("ReadingItemCard", () => {
  test("the reason is named by the card and the fade date stays at rest", () => {
    renderCard(false);
    const card = screen.getByTestId("reading-item");
    const reason = document.getElementById(card.getAttribute("aria-describedby") ?? "");
    expect(reason).toBe(screen.getByTestId("why-here"));
    expect(reason).toHaveTextContent(/^Here because: you subscribed/);
    expect(reason?.textContent).not.toContain("Fades");
    expect(screen.getByTestId("reading-fades")).toHaveTextContent(
      "Fades Wed 21 Oct unless you keep it.",
    );
  });

  test("the reason waits for focus or hover on a wide screen, and shows at rest on a narrow one", () => {
    renderCard(false);
    expect(screen.getByTestId("why-here").className).toContain("sm:hidden");
    expect(screen.getByTestId("why-here").className).toContain("sm:group-hover/card:block");
  });

  test("a focused card shows its reason without the hover rule", () => {
    renderCard(true);
    expect(screen.getByTestId("why-here").className).not.toContain("sm:hidden");
  });

  test("the key hints of a lead card wait for focus or hover", () => {
    renderCard(false, { lead: true });
    expect(screen.getByRole("button", { name: /read/ }).parentElement?.className).toContain(
      "sm:group-hover/card:flex",
    );
  });

  test("a card without a fade date shows its reason unchanged", () => {
    renderCard(true, { why: "Here because: you chose it.", fades: undefined });
    expect(screen.getByTestId("why-here")).toHaveTextContent("Here because: you chose it.");
    expect(screen.queryByTestId("reading-fades")).toBeNull();
  });
});
