/* @vitest-environment jsdom */
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { MessagesList } from "./MessagesList";
import type { MessagesData, MessagesRow } from "./api";

// jsdom has no layout, so the real virtualizer renders nothing. Render every
// row; scrolling isn't what these tests are about.
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: (options: { count: number; getItemKey: (index: number) => string | number }) => ({
    getVirtualItems: () =>
      Array.from({ length: options.count }, (_, index) => ({
        index,
        key: options.getItemKey(index),
        start: index * 52,
        end: index * 52 + 52,
        size: 52,
        lane: 0,
      })),
    getTotalSize: () => options.count * 52,
    measurementsCache: [],
    measureElement: () => undefined,
    scrollToIndex: () => undefined,
  }),
}));

afterEach(cleanup);

function person(id: string, overrides: Partial<MessagesRow> = {}): MessagesRow {
  return {
    id,
    kind: "person",
    account_id: "acc",
    band: "recent",
    title: id,
    closeness: "regular",
    your_turn: false,
    last_at: "2026-10-19T00:00:00Z",
    preview: { kind: "latest", text: `${id} wrote last` },
    topics: [
      {
        account_id: "acc",
        thread_id: `${id}-thread`,
        subject: `${id} subject`,
        shape: "one_to_one",
        state: "quiet",
        last_at: "2026-10-19T00:00:00Z",
        message_count: 1,
        message_ids: [],
        reply_to_message_id: `${id}-m`,
      },
    ],
    overdue: false,
    pinned: false,
    unread: false,
    why: "",
    ...overrides,
  };
}

function renderList(rows: MessagesRow[]) {
  const data: MessagesData = {
    generated_at: "2026-10-19T00:00:00Z",
    header: "Messages",
    your_turn: [],
    pinned: [],
    recent: rows,
    quiet: [],
    recent_total: rows.length,
    quiet_total: 0,
    row_count: rows.length,
    thread_count: rows.length,
    merge_suggestion_count: 0,
  };
  render(
    <MessagesList
      data={data}
      selectedId={null}
      via="key"
      quietOpen={false}
      onToggleQuiet={() => {}}
      scroller={document.body}
      onSelect={() => {}}
      onReply={() => {}}
      onDone={() => {}}
      hidden={new Set()}
      arrival={null}
    />,
  );
}

function rowFor(id: string) {
  const row = screen
    .getAllByTestId("messages-row")
    .find((element) => element.getAttribute("data-row-id") === id);
  if (!row) throw new Error(`no row for ${id}`);
  return row;
}

test("a person with an unsent reply shows Draft: in the accent and the reply in place of the preview", () => {
  renderList([
    person("pat", {
      draft: {
        draft_id: "draft-1",
        thread_id: "pat-thread",
        text: "Friday works, I'll bring the numbers",
        reply_all: false,
        updated_at: "2026-10-19T00:00:00Z",
      },
    }),
    person("sam"),
  ]);

  const line = within(rowFor("pat")).getByTestId("row-preview");
  expect(line).toHaveAttribute("data-kind", "draft");
  expect(line).toHaveTextContent("Draft: Friday works, I'll bring the numbers");
  expect(line).not.toHaveTextContent("pat wrote last");
  // The label is a word, so the mark doesn't rest on colour; the colour is the accent token.
  const label = within(line).getByText("Draft:");
  expect(label).toHaveClass("text-signal");

  const other = within(rowFor("sam")).getByTestId("row-preview");
  expect(other).toHaveTextContent("sam wrote last");
  expect(other).not.toHaveTextContent("Draft:");
});
