/* Fixtures for the arrivals tests: the daemon's words, as it sends them. */

import type { Arrivals, MoveOutcome, NotSure } from "./types";

export function arrivals(overrides: Partial<Arrivals> = {}): Arrivals {
  return {
    generated_at: "2026-10-07T12:00:00Z",
    since: "2026-10-07T08:12:00Z",
    until: "2026-10-07T12:00:01Z",
    since_label: "08:12",
    total: 50,
    counts: [
      { bucket: "messages", count: 8, label: "8 Messages" },
      { bucket: "updates", count: 10, label: "10 Updates" },
      { bucket: "reading", count: 31, label: "31 Reading" },
      { bucket: "spam", count: 1, label: "1 spam" },
    ],
    also: [{ bucket: "todo", count: 2, label: "2 in To do" }],
    line: "Since 08:12: 50 arrived. 8 Messages · 10 Updates · 31 Reading · 1 spam. Also 2 in To do.",
    clear_line: "Clear. All 50 emails since 08:12 are accounted for.",
    never_bury:
      "People you've written to always reach Messages when the mail is addressed to you, whatever list it came through.",
    ...overrides,
  };
}

export function notSure(overrides: Partial<NotSure> = {}): NotSure {
  return {
    account_id: "acct-1",
    message_id: "msg-1",
    thread_id: "thread-1",
    sender_email: "maya@orbit.example",
    sender_name: "Maya Ortiz",
    subject: "Q4 plan",
    mode: "updates",
    line: 'Maya Ortiz copied you on "Q4 plan". Updates for now.',
    choices: [
      { mode: "messages", label: "Messages", key: "m" },
      { mode: "todo", label: "To do", key: "x" },
      { mode: "updates", label: "Updates", key: "u" },
      { mode: "reading", label: "Reading", key: "r" },
      { mode: "archive", label: "Archive", key: "e" },
    ],
    ...overrides,
  };
}

export function outcome(overrides: Partial<MoveOutcome> = {}): MoveOutcome {
  return {
    account_id: "acct-1",
    message_id: "msg-1",
    thread_id: "thread-1",
    sender_email: "maya@orbit.example",
    from: "updates",
    to: "reading",
    sender: false,
    dry_run: false,
    copy: "Moved to Reading.",
    ask_sender: "Always for this sender? (K)",
    hint: "X moves just this email. K sends everything from this sender there from now on.",
    correction_id: 7,
    ...overrides,
  };
}
