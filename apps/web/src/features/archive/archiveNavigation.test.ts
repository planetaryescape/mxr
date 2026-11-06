import { defaultParseSearch } from "@tanstack/react-router";
import { afterEach, expect, test, vi } from "vitest";

import { navigationActions } from "@/lib/actions/navigationActions";
import { setRuntimeNavigate } from "@/lib/actions/runtime";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { archiveResumeHref, useArchiveNavigation } from "./archiveNavigation";

afterEach(() => {
  useArchiveNavigation.setState({ searches: {} });
  useUiPrefs.setState({ accountScope: null });
});

test("Archive shortcut resumes the current account's lookup, not another account's", () => {
  const navigate = vi.fn<(to: string) => void>();
  setRuntimeNavigate({ navigate });
  useArchiveNavigation.getState().remember("work", {
    q: "contract & renewal",
    record: "signed",
    checked: true,
  });
  useArchiveNavigation.getState().remember(null, { q: "lisbon", record: "tap" });
  useUiPrefs.setState({ accountScope: "work" });
  navigationActions.find((action) => action.id === "nav.archive-mode")!.run!({
    path: "/reading",
    activePane: "mailbox",
    scopes: ["global"],
    selectionCount: 0,
    accountCount: 2,
    hasFocusedThread: false,
    isFirstAccountOnly: false,
  });
  const url = new URL(navigate.mock.calls[0]![0], "http://mxr.test");
  expect(defaultParseSearch(url.search)).toEqual({
    q: "contract & renewal",
    record: "signed",
    checked: true,
  });
  useUiPrefs.setState({ accountScope: "unvisited" });
  expect(archiveResumeHref()).toBe("/archive");
  useUiPrefs.setState({ accountScope: null });
  expect(defaultParseSearch(new URL(archiveResumeHref(), "http://mxr.test").search)).toEqual({
    q: "lisbon",
    record: "tap",
  });
});

test("clearing a lookup replaces the remembered question instead of reviving it", () => {
  useArchiveNavigation.getState().remember(null, { q: "old", record: "old-record" });
  useArchiveNavigation.getState().remember(null, {});
  expect(archiveResumeHref()).toBe("/archive");
});
