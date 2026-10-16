/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { ReadingItem } from "./api";
import { ReadingIssueRow, ReadingLinkRow, type RowHandlers } from "./ReadingRows";

function item(extra: Partial<ReadingItem> = {}): ReadingItem {
  return {
    item_key: "m1:0",
    account_id: "a",
    message_id: "m1",
    thread_id: "t1",
    kind: "issue",
    shape: "single",
    title: "A long read",
    standfirst: "The first real paragraph.",
    source: "Long Reads Weekly",
    sender_email: "essays@longreads.example",
    words: 900,
    minutes: 4,
    arrived_at: "2026-10-06T08:00:00Z",
    why: "Here because: you subscribed.",
    on_later: false,
    progress: 0,
    opened: false,
    finished: false,
    article_cached: false,
    ...extra,
  };
}

function handlers(): RowHandlers {
  return {
    onPoint: vi.fn<(key: string) => void>(),
    onFocus: vi.fn<(key: string) => void>(),
    onRead: vi.fn<(key: string) => void>(),
    onLater: vi.fn<(key: string) => void>(),
    onLetGo: vi.fn<(item: ReadingItem) => void>(),
    onUnsubscribe: vi.fn<(item: ReadingItem) => void>(),
    onToggleLinks: vi.fn<(key: string) => void>(),
  };
}

function renderIssue(lit: boolean, extra: Partial<ReadingItem> = {}, h = handlers()) {
  render(
    <ul>
      <ReadingIssueRow
        item={item(extra)}
        index={0}
        lit={lit}
        via="key"
        fading={false}
        handlers={h}
      />
    </ul>,
  );
  return h;
}

describe("ReadingIssueRow", () => {
  test("is a row: the title, the standfirst as its one secondary line, no card", () => {
    renderIssue(false);
    const row = screen.getByTestId("reading-item");
    expect(row.tagName).toBe("LI");
    expect(row).toHaveTextContent("A long read");
    expect(row).toHaveTextContent("The first real paragraph.");
    expect(row).not.toHaveAttribute("data-lit");
  });

  test("the reason and the fade date are not drawn in the row", () => {
    renderIssue(true, { fades: "Fades Wed 21 Oct unless you keep it." });
    expect(screen.queryByTestId("why-here")).toBeNull();
    expect(screen.queryByTestId("reading-fades")).toBeNull();
  });

  test("the lit row shows its verbs; Enter, b and e run read, later and let go", () => {
    const h = renderIssue(true);
    expect(screen.getByTestId("reading-item")).toHaveAttribute("data-lit", "true");
    fireEvent.click(screen.getByTestId("reading-read"));
    fireEvent.click(screen.getByTestId("reading-later"));
    fireEvent.click(screen.getByTestId("reading-let-go"));
    expect(h.onRead).toHaveBeenCalledWith("m1:0");
    expect(h.onLater).toHaveBeenCalledWith("m1:0");
    expect(h.onLetGo).toHaveBeenCalledTimes(1);
  });

  test("unsubscribe is a verb only when the source suggests it", () => {
    const { unmount } = render(
      <ul>
        <ReadingIssueRow
          item={item()}
          index={0}
          lit
          via="key"
          fading={false}
          handlers={handlers()}
        />
      </ul>,
    );
    expect(screen.queryByTestId("reading-row-unsubscribe")).toBeNull();
    unmount();
    renderIssue(true, { unsubscribe_offer: "You haven't read 6 of 6" });
    expect(screen.getByTestId("reading-row-unsubscribe")).toBeInTheDocument();
  });

  test("the headline is a link that reads the issue", () => {
    const h = renderIssue(false);
    fireEvent.click(screen.getByRole("link", { name: /A long read/ }));
    expect(h.onRead).toHaveBeenCalledWith("m1:0");
  });

  test("a pointer press chooses the row with the pointer", () => {
    const h = renderIssue(false);
    fireEvent.mouseDown(screen.getByTestId("reading-item"));
    expect(h.onPoint).toHaveBeenCalledWith("m1:0");
  });
});

describe("ReadingLinkRow", () => {
  test("a digest link is a row under its issue with its site in the trail", () => {
    render(
      <ul>
        <ReadingLinkRow
          link={{
            item_key: "m1:1",
            title: "SQLite internals",
            domain: "sqlite.org",
            tracked: false,
            article_cached: false,
            on_later: false,
            url: "https://sqlite.org/x",
          }}
          index={1}
          lit={false}
          via="key"
          handlers={handlers()}
        />
      </ul>,
    );
    const row = screen.getByTestId("reading-link");
    expect(row).toHaveTextContent("SQLite internals");
    expect(row).toHaveTextContent("sqlite.org");
  });
});
