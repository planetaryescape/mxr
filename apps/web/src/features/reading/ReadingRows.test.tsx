/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { ReadingItem } from "./api";
import { ReadingIssueRow, type RowHandlers } from "./ReadingRows";

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

});

describe("ReadingLinkRow", () => {
});
