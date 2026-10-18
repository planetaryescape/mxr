import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { resetDaemonAvailabilityForTest } from "@/lib/daemonAvailability";
import { setToken } from "@/lib/tokenStorage";

import type { MicrosoftTodoLink } from "./api";
import { MicrosoftTodoAction } from "./MicrosoftTodoAction";
import { todoFixture } from "./testing";

beforeEach(() => {
  resetDaemonAvailabilityForTest();
  setToken("test-token");
});
afterEach(() => vi.unstubAllGlobals());

const preview: MicrosoftTodoLink = {
  dry_run: true,
  todo_id: "todo_bill",
  title: "Pay council tax",
  due: "2026-11-05",
  scheduled: null,
  notes: "From Mixer\nPayment due\nMixer to-do: todo_bill",
  fingerprint: "preview-fingerprint",
  state: "preview",
  task_id: null,
  summary: "Title, due date, schedule and completion will sync both ways.",
};

function setup(installed: boolean, unknown = false, statusFails = false) {
  const commits: unknown[] = [];
  const fetch = vi.fn<typeof globalThis.fetch>(async (input, init) => {
    const path = String(input);
    if (path.endsWith("/todos/microsoft")) {
      if (statusFails) {
        return Response.json({ error: "Status request refused" }, { status: 403 });
      }
      return Response.json({
        kind: "MicrosoftTodoStatus",
        status: {
          installed,
          install_url: "https://github.com/planetaryescape/ms-todo#installation",
          hint: "Install ms-todo",
        },
      });
    }
    if (path.endsWith("/todos/microsoft/sync")) {
      return Response.json({
        kind: "MicrosoftTodoSync",
        sync: {
          dry_run: false,
          results: [{ todo_id: "todo_bill", state: "synced", summary: "Both tasks agree." }],
        },
      });
    }
    const body: unknown = JSON.parse(String(init?.body));
    if (typeof body === "object" && body !== null && "dry_run" in body && body.dry_run === false) {
      commits.push(body);
      return Response.json({
        kind: "MicrosoftTodoSend",
        result: {
          ...preview,
          dry_run: false,
          state: "linked",
          task_id: "remote-task",
          summary: "Linked. Shared fields sync both ways.",
        },
      });
    }
    return Response.json({
      kind: "MicrosoftTodoSend",
      result: unknown
        ? {
            ...preview,
            state: "unknown",
            summary: "The handoff is unconfirmed. Check ms-todo's outbox.",
          }
        : preview,
    });
  });
  vi.stubGlobal("fetch", fetch);
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MicrosoftTodoAction todo={todoFixture()} />
    </QueryClientProvider>,
  );
  return { fetch, commits };
}

test("without the CLI it offers installation, never a send button", async () => {
  const { fetch, commits } = setup(false);
  expect(await screen.findByRole("link", { name: "Install ms-todo" })).toHaveAttribute(
    "href",
    "https://github.com/planetaryescape/ms-todo#installation",
  );
  expect(screen.queryByRole("button", { name: "Microsoft To Do" })).toBeNull();
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(commits).toHaveLength(0);
});

test("linking requires a visible exact preview and sends its fingerprint once", async () => {
  const { commits } = setup(true);
  fireEvent.click(await screen.findByRole("button", { name: "Microsoft To Do" }));
  const confirm = await screen.findByRole("button", { name: "Link and sync both ways" });
  expect(screen.getByText("Pay council tax")).toBeVisible();
  expect(screen.getByText("2026-11-05")).toBeVisible();
  expect(screen.getByText(/Email bodies and attachments are not sent/)).toBeVisible();
  expect(commits).toHaveLength(0);
  fireEvent.click(confirm);
  fireEvent.click(confirm);
  await waitFor(() =>
    expect(commits).toEqual([
      { account_id: todoFixture().account_id, dry_run: false, fingerprint: "preview-fingerprint" },
    ]),
  );
  expect(await screen.findByRole("button", { name: "Sync now" })).toBeVisible();
});

test("an uncertain receipt cannot create another Microsoft task", async () => {
  const { commits } = setup(true, true);
  fireEvent.click(await screen.findByRole("button", { name: "Microsoft To Do" }));
  expect(await screen.findByText(/handoff is unconfirmed/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Link and sync both ways" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Sync now" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Refresh link status" }));
  expect(commits).toHaveLength(0);
});

test("status errors stay visible and offer a safe read-only retry", async () => {
  const { fetch, commits } = setup(true, false, true);
  expect(await screen.findByRole("alert")).toHaveTextContent("Status request refused");
  expect(screen.queryByRole("button", { name: "Microsoft To Do" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Retry Microsoft To Do status" }));
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
  expect(commits).toHaveLength(0);
  expect(fetch.mock.calls.every(([, init]) => init?.method === "GET")).toBe(true);
});
