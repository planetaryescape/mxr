/* @vitest-environment jsdom */

import { render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, test, vi } from "vitest";

import type { RuntimeAccount } from "@/features/compose/api";
import { AccountIdentitiesContext, identitiesFrom } from "@/features/accounts/useAccountIdentities";
import type { components } from "@/api/generated";

import { PersonRow, TodoRow, UpdatesRow } from "./NowRows";
import type { NowItem } from "./nowItems";

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children, ...rest }: { children: ReactNode }) => <a {...rest}>{children}</a>,
}));

type Schemas = components["schemas"];

const ACCOUNTS: RuntimeAccount[] = [
  {
    account_id: "work",
    name: "Work",
    email: "bk@work.example",
    provider_kind: "imap",
    is_default: true,
    enabled: true,
    identity: { color: "sky", tag: "W" },
  },
  {
    account_id: "home",
    name: "Home",
    email: "bk@home.example",
    provider_kind: "imap",
    is_default: false,
    enabled: true,
    identity: { color: "amber", tag: "H" },
  },
];

const identities = identitiesFrom(ACCOUNTS, false);

function state(index: number) {
  return {
    index,
    lit: false,
    via: "key" as const,
    leaving: false,
    onSelect: vi.fn<() => void>(),
    onOpen: vi.fn<() => void>(),
    onDone: vi.fn<() => void>(),
  };
}

/** Only the fields the Now rows read; the cast stands in for the full daemon payload. */
function deskRow(account_id: string, counterparty_name: string): Schemas["DeskRowData"] {
  return {
    account_id,
    counterparty_email: `${counterparty_name.toLowerCase()}@example.com`,
    counterparty_name,
    lane: "owed",
    since: "2026-10-20T10:00:00Z",
    age_seconds: 3600,
    subject: "Quick question",
    unread: false,
    thread_id: `thread-${account_id}-${counterparty_name}`,
    message_id: `message-${account_id}-${counterparty_name}`,
  } as unknown as Schemas["DeskRowData"];
}

function todo(account_id: string, title: string): Schemas["TodoData"] {
  return {
    id: `todo-${account_id}-${title}`,
    account_id,
    title,
    when_label: "Fri 24 Oct",
    counterparty: null,
    person_label: null,
    unread: false,
  } as unknown as Schemas["TodoData"];
}

describe("Now rows show each row's own account", () => {
  test("person rows from two accounts each carry their own tag", () => {
    const rows: NowItem[] = [
      {
        kind: "person",
        key: "p1",
        threadId: "t1",
        person: { row: deskRow("work", "Ada"), why: "your turn", new_sender: null },
      } as NowItem,
      {
        kind: "person",
        key: "p2",
        threadId: "t2",
        person: { row: deskRow("home", "Ben"), why: "your turn", new_sender: null },
      } as NowItem,
    ];
    render(
      <AccountIdentitiesContext.Provider value={identities}>
        <ul>
          {rows.map((item, index) => (
            <PersonRow
              key={item.key}
              item={item as never}
              {...state(index)}
              onReply={vi.fn<() => void>()}
            />
          ))}
        </ul>
      </AccountIdentitiesContext.Provider>,
    );
    const [first, second] = screen.getAllByTestId("now-row");
    expect(within(first!).getByRole("img", { name: "Work" })).toHaveTextContent("W");
    expect(within(second!).getByRole("img", { name: "Home" })).toHaveTextContent("H");
  });

  test("to-do rows from two accounts each carry their own tag", () => {
    const rows = [
      {
        kind: "todo",
        key: "d1",
        threadId: null,
        todo: { todo: todo("work", "File the return"), why: "due" },
      },
      {
        kind: "todo",
        key: "d2",
        threadId: null,
        todo: { todo: todo("home", "Renew passport"), why: "due" },
      },
    ] as NowItem[];
    render(
      <AccountIdentitiesContext.Provider value={identities}>
        <ul>
          {rows.map((item, index) => (
            <TodoRow key={item.key} item={item as never} {...state(index)} />
          ))}
        </ul>
      </AccountIdentitiesContext.Provider>,
    );
    const [first, second] = screen.getAllByTestId("now-row");
    expect(within(first!).getByRole("img", { name: "Work" })).toHaveTextContent("W");
    expect(within(second!).getByRole("img", { name: "Home" })).toHaveTextContent("H");
  });

  test("the Updates card wears no chip, since it can span accounts", () => {
    const item = {
      kind: "updates",
      key: "u1",
      card: {
        headline: "3 updates from GitHub",
        line: "3 updates",
        since: "2026-10-20T10:00:00Z",
        lines: [],
      },
    } as unknown as NowItem;
    render(
      <AccountIdentitiesContext.Provider value={identities}>
        <ul>
          <UpdatesRow item={item as never} {...state(0)} onLetGo={vi.fn<() => void>()} />
        </ul>
      </AccountIdentitiesContext.Provider>,
    );
    expect(screen.queryByRole("img", { name: /Work|Home/ })).toBeNull();
  });
});
