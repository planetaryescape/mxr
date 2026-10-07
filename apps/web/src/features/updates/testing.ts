/* Fixtures for Updates' tests, shaped like the daemon's digest. */

import type { UpdateLine, UpdatesDigest } from "./api";

export function lineFixture(overrides: Partial<UpdateLine> = {}): UpdateLine {
  return {
    id: "acc|strava.com|source",
    section: "changed",
    account_id: "acc",
    source_key: "strava.com",
    source_name: "Strava",
    sender_email: "no-reply@strava.com",
    fact: "Your week in running: 21.3 km over 3 runs",
    fact_source: "subject",
    numbers: [{ raw: "21.3 km", value: 21.3, unit: "km" }],
    delta: {
      raw: "21.3 km",
      previous_raw: "19.0 km",
      change: 12.1,
      text: "up 12% on last week",
      against: "2026-09-30T05:00:00Z",
    },
    signal: "changed",
    count: 1,
    message_ids: ["m1"],
    thread_ids: ["t1"],
    latest_message_id: "m1",
    latest_thread_id: "t1",
    latest_at: "2026-10-07T05:00:00Z",
    link: { url: "https://www.strava.com/athlete/training", domain: "strava.com" },
    todo_title: "Check Strava: Your week in running",
    why: "Here because: automated sender (rule). In the 08:00 digest.",
    setting: "every_digest",
    ...overrides,
  };
}

export function digestFixture(overrides: Partial<UpdatesDigest> = {}): UpdatesDigest {
  const routine = Array.from({ length: 6 }, (_, at) =>
    lineFixture({
      id: `acc|source${at}|source`,
      section: "routine",
      signal: "routine",
      source_key: `source${at}.example`,
      source_name: `Source ${at}`,
      count: at + 1,
      delta: undefined,
    }),
  );
  return {
    generated_at: "2026-10-07T09:00:00Z",
    header: "Notifications gathered twice a day. Read the digest, then let go.",
    cut: {
      at: "2026-10-07T08:00:00Z",
      label: "08:00",
      title: "This morning's digest",
      previous_at: "2026-10-06T16:30:00Z",
      next_at: "2026-10-07T16:30:00Z",
      next_label: "16:30",
      cuts: ["08:00", "16:30"],
    },
    headline: "1 needs a look, 1 changed. 21 routine from 6 sources.",
    message_count: 23,
    source_count: 8,
    needs_a_look: [
      lineFixture({
        id: "acc|google.com|needs",
        section: "needs_a_look",
        signal: "needs_you",
        source_key: "google.com",
        source_name: "Google",
        fact: "Security alert: New sign-in from Chrome on Windows",
        delta: undefined,
        link: undefined,
        in_todo: "already in To do",
        todo_id: "todo_1",
      }),
    ],
    changed: [lineFixture()],
    routine,
    since: { label: "arriving for 16:30", message_count: 0, source_count: 0, lines: [] },
    expired_count: 0,
    selection_token: "token",
    let_go_line: "Let go of 23 updates from 8 sources; 1 also in To do stays there.",
    source_total: 8,
    muted_total: 0,
    ...overrides,
  };
}
