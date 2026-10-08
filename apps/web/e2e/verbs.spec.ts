import { expect, test, type Page } from "@playwright/test";

import { VERB_FEEDBACK, type Verb } from "../src/features/mail-actions/verbFeedback";
import {
  cursorRowId,
  cursorTo,
  mailList,
  mailRows,
  modKey,
  openList,
  rowById,
  rowLabel,
  rowStates,
} from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

/*
 * The undo matrix (rubric B3): one journey per verb in the verb table. An
 * undoable verb runs, its toast starts with the table's words, and its undo
 * path restores what changed. An irreversible verb must say why in the
 * table and confirm first; its journey checks the confirm. A verb added to
 * the table without a journey fails below.
 */

type Journey = (page: Page) => Promise<void>;

/** The newest toast whose title matches `words` (its buttons aren't part of the match). */
function toast(page: Page, words: string | RegExp) {
  const title = page.locator("[data-title]").filter({ hasText: words });
  return page.locator("[data-sonner-toast]").filter({ has: title }).last();
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** The result toast for `verb`, in the table's words. */
async function expectToast(page: Page, verb: Verb): Promise<void> {
  const words = new RegExp(`^${escapeRegExp(VERB_FEEDBACK[verb].pastTense)}\\b`);
  await expect(toast(page, words), verb).toBeVisible();
  // In the verb's colour: green for done, the accent for a change with undo.
  await expect(toast(page, words), verb).toHaveAttribute("data-type", VERB_FEEDBACK[verb].tone);
}

/** The verb's toast, its row gone, then `u` and the row back. */
async function leavesThenUndoes(page: Page, verb: Verb, rowId: string): Promise<void> {
  await expectToast(page, verb);
  await expect(rowById(page, rowId)).toHaveCount(0);
  await page.keyboard.press("u");
  await expectUndone(page);
  await expect(rowById(page, rowId)).toBeVisible();
}

/** The undo's own toast: "Undone", or "Snooze undone" for a wake. */
async function expectUndone(page: Page): Promise<void> {
  await expect(toast(page, /^(Snooze u|U)ndone$/)).toBeVisible();
}

const isStarred = (label: string) => rowStates(label).includes("Starred.");
const isUnread = (label: string) => rowStates(label).includes("Unread.");
// Snooze acts on messages; a conversation with other messages stays in the
// inbox, so it takes a single-message row.
const isSingleMessage = (label: string) => !/messages in conversation/.test(label);

/**
 * A verb on the inbox row under the cursor, from its key: the row leaves
 * (or changes state), the toast says so, and the table's undo path puts it
 * back as it was with `u`.
 */
function rowVerb(
  verb: Verb,
  keys: string[],
  options: { pick?: (label: string) => boolean; leaves: boolean },
): Journey {
  return async (page) => {
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    const rowId = await cursorTo(page, options.pick ?? (() => true));
    const before = rowStates(await rowLabel(page, rowId));
    for (const key of keys) {
      await page.keyboard.press(key);
      // A dialog verb (snooze) takes its next key once the dialog is up.
      if (key === "Z")
        await expect(page.getByRole("dialog", { name: "Snooze until…" })).toBeVisible();
    }
    await expectToast(page, verb);
    if (options.leaves) await expect(rowById(page, rowId)).toHaveCount(0);
    else await expect.poll(async () => rowStates(await rowLabel(page, rowId))).not.toEqual(before);

    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(rowById(page, rowId)).toBeVisible();
    await expect.poll(async () => rowStates(await rowLabel(page, rowId))).toEqual(before);
  };
}

function labelsDialog(page: Page) {
  return page.getByRole("dialog", { name: "Labels" });
}

/** Toggle labels in the labels dialog, one by one, then apply. */
async function changeLabels(page: Page, names: string[]): Promise<void> {
  await page.keyboard.press("l");
  const dialog = labelsDialog(page);
  const input = dialog.getByPlaceholder("Find or create a label…");
  await expect(input).toBeFocused();
  for (const name of names) {
    await input.fill(name);
    await page.keyboard.press("Enter");
    await expect(
      dialog.getByRole("option", { name: new RegExp(`^${name}: .*changed$`) }),
    ).toBeVisible();
  }
  await dialog.getByRole("button", { name: /^Apply/ }).click();
  await expect(dialog).toHaveCount(0);
}

/** A conversation in the Hiring label's list: removing Hiring takes it out. */
async function openHiringRow(page: Page): Promise<string> {
  await openList(page, "/m/label/Hiring");
  await mailList(page).focus();
  return cursorRowId(page);
}

function labelChange(verb: Verb, names: string[]): Journey {
  return async (page) => {
    const rowId = await openHiringRow(page);
    await changeLabels(page, names);
    await leavesThenUndoes(page, verb, rowId);
  };
}

interface Bundle {
  account_id: string;
  sender_email: string;
  message_count: number;
  messages: { message_id: string; pinned: boolean }[];
}

async function placeBundles(page: Page, slug: "reading"): Promise<Bundle[]> {
  const place = await bridge<{ bundles: Bundle[] }>(
    page,
    `/api/v1/mail/places/${slug}?messages_per_bundle=50`,
  );
  return place.bundles;
}

function composer(page: Page) {
  return page.getByRole("dialog", { name: "New message" });
}

/** The composer body is CodeMirror in vim mode: insert, type, back to normal. */
async function typeBody(page: Page, text: string) {
  await composer(page).locator(".cm-content").click();
  await page.keyboard.press("i");
  await page.keyboard.type(text);
  await page.keyboard.press("Escape");
}

interface Row {
  id: string;
  thread_id: string;
  subject: string;
  starred: boolean;
  message_ids?: string[];
}

async function starredIds(page: Page, threadId: string): Promise<string[]> {
  const thread = await bridge<{ messages: { id: string; starred: boolean }[] }>(
    page,
    `/api/v1/mail/threads/${encodeURIComponent(threadId)}`,
  );
  return thread.messages.filter((message) => message.starred).map((message) => message.id);
}

/**
 * An inbox row in the first rows that is starred by a message outside the
 * row's own list (the demo's "Launch checklist for Project Aurora": an
 * inbox message and a sent reply, both starred). Earlier specs can leave
 * that thread changed, so when no row is in that state, this stars both
 * messages of the first inbox row with a message outside its list.
 */
async function starredOutsideTheList(page: Page): Promise<Row> {
  const path = "/api/v1/mail/mailbox?lens_kind=inbox&view=threads&limit=60&offset=0";
  const rows = async () =>
    (await bridge<{ mailbox: { groups: { rows: Row[] }[] } }>(page, path)).mailbox.groups.flatMap(
      (group) => group.rows,
    );
  for (const row of await rows()) {
    if (!row.starred) continue;
    const listed = new Set(row.message_ids ?? [row.id]);
    const starred = await starredIds(page, row.thread_id);
    if (starred.some((id) => !listed.has(id))) return row;
  }
  for (const row of await rows()) {
    const listed = new Set(row.message_ids ?? [row.id]);
    const thread = await bridge<{ messages: { id: string }[] }>(
      page,
      `/api/v1/mail/threads/${encodeURIComponent(row.thread_id)}`,
    );
    const outside = thread.messages.find((message) => !listed.has(message.id));
    if (!outside) continue;
    await bridge(page, "/api/v1/mail/mutations/star", {
      message_ids: [...listed, outside.id],
      starred: true,
    });
    return { ...row, starred: true };
  }
  throw new Error("the demo inbox has no conversation with a message outside its row");
}

/** To do's runway with `title` under the cursor, once the demo's first run has it. */
async function todoRow(page: Page, title: string, path = "/todo") {
  await expect
    .poll(
      async () =>
        JSON.stringify(
          await bridge(
            page,
            path === "/todo" ? "/api/v1/mail/todos" : "/api/v1/mail/todos/catchup",
          ),
        ),
      { timeout: 60_000 },
    )
    .toContain(title);
  await openApp(page, path);
  const row = page.getByTestId("todo-row").filter({ hasText: title }).first();
  await row.getByText(title, { exact: false }).first().click();
  await expect(row).toHaveAttribute("aria-current", "true");
  return row;
}

/** A To do verb from its key: the toast in the table's words, then `u` restores. */
function todoVerb(verb: Verb, key: string, title = "Fix payment for Spotify"): Journey {
  return async (page) => {
    await todoRow(page, title);
    await page.keyboard.press(key);
    await expectToast(page, verb);
    await expect(page.getByTestId("todo-row").filter({ hasText: title })).toHaveCount(0);
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(page.getByTestId("todo-row").filter({ hasText: title }).first()).toBeVisible();
  };
}

/** A catch-up decision: the row leaves the batch, and `u` puts it back undecided. */
function catchupVerb(verb: Verb, key: string): Journey {
  return async (page) => {
    const title = "Pay water bill";
    await todoRow(page, title, "/todo?view=catchup");
    await page.keyboard.press(key);
    await expectToast(page, verb);
    await expect(page.getByTestId("todo-row").filter({ hasText: title })).toHaveCount(0);
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(page.getByTestId("todo-row").filter({ hasText: title })).toBeVisible();
  };
}

/** An Archive verb on the Apple receipt, answered from the ask box, then `u`. */
function recordVerb(verb: Verb, act: (page: Page) => Promise<void>): Journey {
  return async (page) => {
    await openApp(page, "/archive");
    await page.getByTestId("archive-ask").fill("apple");
    await expect(page.getByTestId("answer-card")).toContainText("Apple");
    await page.keyboard.press("Enter");
    await act(page);
    await expectToast(page, verb);
    await page.keyboard.press("u");
    await expectUndone(page);
  };
}

/**
 * X on an Inbox row: the picker, one key, the move's toast, the row's mode
 * chip naming the new mode, and `u` putting it back where it was.
 */
const modeMove: Journey = async (page) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
  const rowId = await cursorTo(page, () => true);
  const chip = rowById(page, rowId).getByTestId("mode-chip");
  await expect(chip).toBeVisible();
  const before = (await chip.textContent()) ?? "";
  const target = before === "Reading" ? "u" : "r";
  await page.keyboard.press("X");
  await expect(page.getByTestId("move-to-mode-dialog")).toBeVisible();
  await page.keyboard.press(target);
  await expectToast(page, "mode-move");
  await expect(chip).toHaveText(target === "r" ? "Reading" : "Updates");
  await page.keyboard.press("u");
  await expect(toast(page, /^Moved back to /)).toBeVisible();
  await expect(chip).toHaveText(before);
};

const JOURNEYS: Partial<Record<Verb, Journey>> = {
  "mode-move": modeMove,
  archive: rowVerb("archive", ["e"], { leaves: true }),
  "read-and-archive": rowVerb("read-and-archive", ["m"], { leaves: true }),
  trash: rowVerb("trash", ["#"], { leaves: true }),
  spam: rowVerb("spam", ["!"], { leaves: true }),
  star: rowVerb("star", ["s"], { pick: (label) => !isStarred(label), leaves: false }),
  unstar: async (page) => {
    // A conversation starred by a message outside the inbox too (a starred
    // reply in Sent): the row's unstar clears both, and `u` brings both back.
    const row = await starredOutsideTheList(page);
    const before = await starredIds(page, row.thread_id);
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    const rowId = await cursorTo(page, (label) => label.includes(row.subject));
    expect(isStarred(await rowLabel(page, rowId))).toBe(true);

    await page.keyboard.press("s");
    await expectToast(page, "unstar");
    await expect.poll(async () => isStarred(await rowLabel(page, rowId))).toBe(false);
    expect(await starredIds(page, row.thread_id)).toEqual([]);

    await page.keyboard.press("u");
    await expectUndone(page);
    await expect.poll(async () => isStarred(await rowLabel(page, rowId))).toBe(true);
    expect((await starredIds(page, row.thread_id)).sort()).toEqual(before.sort());
  },
  read: rowVerb("read", ["I"], { pick: isUnread, leaves: false }),
  unread: rowVerb("unread", ["U"], { pick: (label) => !isUnread(label), leaves: false }),
  snooze: rowVerb("snooze", ["Z", "1"], { pick: isSingleMessage, leaves: true }),

  move: async (page) => {
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    const rowId = await cursorRowId(page);
    await page.keyboard.press("v");
    const dialog = page.getByRole("dialog", { name: "Move to" });
    await dialog.getByPlaceholder("Destination label…").fill("Travel");
    await dialog.getByRole("option", { name: "Travel" }).click();
    await leavesThenUndoes(page, "move", rowId);
  },

  route: async (page) => {
    // A label's list is a queue: route takes the row out of it into another.
    const rowId = await openHiringRow(page);
    await page.keyboard.press(`${await modKey(page)}+k`);
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("combobox").fill("Route out of this queue");
    await palette.getByRole("option", { name: /Route out of this queue/ }).click();
    const dialog = page.getByRole("dialog", { name: "Route out of Hiring" });
    await dialog.getByPlaceholder("Destination label…").fill("Travel");
    await dialog.getByRole("option", { name: "Travel" }).click();
    // A conversation of several messages previews the route first.
    const confirm = page.getByRole("button", { name: /^Route / });
    if (await confirm.isVisible()) await confirm.click();
    await leavesThenUndoes(page, "route", rowId);
  },

  "label-add": async (page) => {
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    // A row without the Hiring chip (labels.spec may have added it to some).
    let rowId = await cursorRowId(page);
    for (let step = 0; step < 25; step += 1) {
      if ((await rowById(page, rowId).getByText("Hiring", { exact: true }).count()) === 0) break;
      await page.keyboard.press("j");
      rowId = await cursorRowId(page);
    }
    const chip = rowById(page, rowId).getByText("Hiring", { exact: true });
    await expect(chip).toHaveCount(0);
    await changeLabels(page, ["Hiring"]);
    await expectToast(page, "label-add");
    await expect(chip).toBeVisible();
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(chip).toHaveCount(0);
  },
  "label-remove": labelChange("label-remove", ["Hiring"]),
  labels: labelChange("labels", ["Hiring", "Travel"]),

  "reply-later": async (page) => {
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    const queued = async () =>
      (await bridge<{ messages: unknown[] }>(page, "/api/v1/mail/reply-later")).messages.length;
    const before = await queued();
    // `b` asks when; Enter with no time is the plain reply later.
    await page.keyboard.press("b");
    await expect(page.getByRole("dialog", { name: "Reply later" })).toBeVisible();
    await page.keyboard.press("Enter");
    await expectToast(page, "reply-later");
    await expect.poll(queued).toBe(before + 1);
    await toast(page, VERB_FEEDBACK["reply-later"].pastTense)
      .getByRole("button", { name: "Undo" })
      .click();
    await expect.poll(queued).toBe(before);
  },

  "reply-later-at": async (page) => {
    await openApp(page, "/desk?lane=owed");
    await expect(mailRows(page).first()).toBeVisible();
    const rowId = await cursorRowId(page);
    await page.keyboard.press("b");
    const dialog = page.getByRole("dialog", { name: "Reply later" });
    await expect(dialog).toBeVisible();
    await page.keyboard.type("in 2d");
    await expect(dialog.getByRole("status")).toContainText(/· in 2 days$/);
    await page.keyboard.press("Enter");
    await leavesThenUndoes(page, "reply-later-at", rowId);
  },

  send: async (page) => {
    test.setTimeout(90_000);
    let sends = 0;
    page.on("request", (request) => {
      if (request.url().includes("/api/v1/mail/compose/session/send")) sends += 1;
    });
    const subject = `e2e-undo-send-${Date.now().toString(36)}`;
    await openList(page, "/m/inbox");
    await page.keyboard.press("c");
    await composer(page).getByRole("combobox", { name: "To" }).fill("alice@example.com");
    await composer(page).getByRole("textbox", { name: "Subject" }).fill(subject);
    await typeBody(page, "Undo me inside the window");
    const sendButton = composer(page).getByRole("button", { name: /^Send (⌘|Ctrl)/ });
    await sendButton.click();

    // Inside the window, Undo holds the send and the draft is still there.
    const countdown = toast(page, /^Sending in \d+s/);
    await expect(countdown).toBeVisible();
    await countdown.getByRole("button", { name: "Undo" }).click();
    await expect(page.getByText("Send cancelled")).toBeVisible();
    await expect(composer(page).getByRole("textbox", { name: "Subject" })).toHaveValue(subject);
    await expect(composer(page).locator(".cm-content")).toContainText("Undo me inside the window");

    // Sent for real this time: only this send leaves, after its own window
    // (which closes after the cancelled one's would have).
    const sent = page.waitForResponse("**/api/v1/mail/compose/session/send", { timeout: 30_000 });
    await sendButton.click();
    expect((await sent).ok()).toBe(true);
    await expectToast(page, "send");
    expect(sends).toBe(1);
  },

  unsubscribe: async (page) => {
    // Irreversible, so it must confirm first. Nothing reaches the daemon
    // until the dialog is answered, and Esc answers no.
    const requests: string[] = [];
    await page.route("**/api/v1/mail/actions/unsubscribe", async (route) => {
      requests.push(route.request().url());
      await route.fulfill({ json: { ok: true } });
    });
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    await page.keyboard.press("D");
    const dialog = page.getByRole("dialog", { name: /^Unsubscribe from .+\?$/ });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText(/@/).first()).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    expect(requests).toEqual([]);

    // Confirmed with u: one request, the table's words, and no Undo offered.
    await page.keyboard.press("D");
    await expect(dialog).toBeVisible();
    await dialog.press("u");
    await expect(dialog).toHaveCount(0);
    await expectToast(page, "unsubscribe");
    await expect(
      toast(page, VERB_FEEDBACK.unsubscribe.pastTense).getByRole("button", { name: "Undo" }),
    ).toHaveCount(0);
    expect(requests).toHaveLength(1);
  },

  "desk-done": async (page) => {
    await openApp(page, "/desk");
    await expect(mailRows(page).first()).toBeVisible();
    await mailList(page).focus();
    const rowId = await cursorRowId(page);
    await page.keyboard.press("e");
    await leavesThenUndoes(page, "desk-done", rowId);
  },

  "mode-done": async (page) => {
    // Noor's one conversation: done here takes her out of Your turn.
    const noor = "person:noor@tidewater.example";
    await openApp(page, `/messages?person=${encodeURIComponent(noor)}`);
    const row = page.getByTestId("band-your_turn").locator(`[data-row-id="${noor}"]`);
    try {
      await expect(row).toBeVisible();
    } catch (error) {
      // Noor is a shared fixture: say where she is and what moved, so a
      // failure here names its cause instead of only its symptom.
      for (const path of ["/api/v1/mail/people", "/api/v1/mail/corrections?limit=100"]) {
        // oxlint-disable-next-line no-await-in-loop
        const body = JSON.stringify(await bridge<unknown>(page, path));
        console.log(`[mode-done] ${path}: ${body.slice(0, 20_000)}`);
        // oxlint-disable-next-line no-await-in-loop
        await test
          .info()
          .attach(path.replace(/\W+/g, "-"), { body, contentType: "application/json" });
      }
      throw error;
    }
    await expect(page.getByTestId("conversation")).toBeVisible();
    await page.keyboard.press("e");
    await expectToast(page, "mode-done");
    await expect(row).toHaveCount(0);
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(row).toBeVisible();
  },

  "digest-let-go": async (page) => {
    await openApp(page, "/now");
    const card = page.getByTestId("now-section-updates");
    await expect(card).toBeVisible();
    await page.keyboard.press("A");
    const dialog = page.getByTestId("updates-let-go-dialog");
    await expect(dialog).toContainText(/^Let go of \d+ updates? from/);
    await dialog.getByRole("button", { name: "Let go", exact: true }).click();
    await expectToast(page, "digest-let-go");
    await expect(card).toHaveCount(0);
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(page.getByTestId("now-section-updates")).toBeVisible();
  },

  "todo-done": todoVerb("todo-done", "e"),
  "todo-dismiss": todoVerb("todo-dismiss", "X"),

  "todo-schedule": async (page) => {
    const row = await todoRow(page, "Fix payment for Spotify");
    await page.keyboard.press("Z");
    const dialog = page.getByRole("dialog", { name: "Schedule" });
    await dialog.getByRole("textbox").fill("in 3 days");
    await expect(dialog.getByRole("button", { name: "Schedule" })).toBeEnabled();
    await dialog.getByRole("button", { name: "Schedule" }).click();
    await expectToast(page, "todo-schedule");
    await expect(row).toHaveAttribute("data-band", "coming");
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(
      page.getByTestId("todo-row").filter({ hasText: "Fix payment for Spotify" }),
    ).toHaveAttribute("data-band", "now");
  },

  "todo-edit": async (page) => {
    await todoRow(page, "Fix payment for Spotify");
    await page.keyboard.press(",");
    const dialog = page.getByRole("dialog", { name: "Edit to-do" });
    await dialog.getByLabel("What to do").fill("Update the Spotify card");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expectToast(page, "todo-edit");
    await expect(
      page.getByTestId("todo-title").filter({ hasText: "Update the Spotify card" }),
    ).toBeVisible();
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(
      page.getByTestId("todo-title").filter({ hasText: "Fix payment for Spotify" }),
    ).toBeVisible();
  },

  "todo-create": async (page) => {
    await openList(page, "/m/inbox");
    await mailRows(page).first().click();
    await page.keyboard.press("t");
    const dialog = page.getByRole("dialog", { name: "Make a to-do" });
    await dialog.getByLabel("What to do").fill("Check the verbs table");
    await dialog.getByRole("button", { name: "Add to To do" }).click();
    await expectToast(page, "todo-create");
    await page.keyboard.press("u");
    await expectUndone(page);
    const runway = JSON.stringify(await bridge(page, "/api/v1/mail/todos"));
    expect(runway).not.toContain("Check the verbs table");
  },

  "todo-keep": catchupVerb("todo-keep", "Enter"),
  "todo-let-go": catchupVerb("todo-let-go", "e"),

  "record-file": async (page) => {
    await openList(page, "/m/inbox");
    await mailList(page).focus();
    await page.keyboard.press("T");
    const dialog = page.getByTestId("pass-to-mode-dialog");
    await expect(dialog.getByTestId("file-preview")).toContainText("Would file in Archive");
    await dialog.getByRole("button", { name: "File in Archive" }).click();
    await expectToast(page, "record-file");
    await page.keyboard.press("u");
    await expectUndone(page);
  },
  "record-dismiss": recordVerb("record-dismiss", async (page) => {
    await page.keyboard.press("X");
  }),
  "record-check": recordVerb("record-check", async (page) => {
    await page.keyboard.press("v");
  }),
  "record-fix": recordVerb("record-fix", async (page) => {
    await page.keyboard.press(",");
    const dialog = page.getByTestId("record-edit-dialog");
    await dialog.getByRole("textbox").fill("£3.49");
    await expect(dialog.getByTestId("record-edit-preview")).toContainText("£3.49");
    await dialog.getByRole("button", { name: "Save" }).click();
  }),

  "reading-later": async (page) => {
    await openApp(page, "/reading");
    const item = page
      .getByTestId("reading-item")
      .filter({ hasNot: page.getByText("On Later") })
      .first();
    const key = await item.getAttribute("data-key");
    if (!key) throw new Error("the demo edition has no item off Later");
    const saved = page.locator(`[data-testid='reading-item'][data-key='${key}']`).first();
    await saved.click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("b");
    await expectToast(page, "reading-later");
    await expect(saved.getByText("On Later")).toBeVisible();
    await page.keyboard.press("u");
    await expectUndone(page);
    await expect(saved.getByText("On Later")).toHaveCount(0);
  },

  "move-sender": async (page) => {
    const [bundle] = await placeBundles(page, "reading");
    if (!bundle) throw new Error("the demo has no Reading bundle to move");
    try {
      await openApp(page, "/reading");
      const issue = page.locator(
        `[data-testid='reading-item'][data-sender='${bundle.sender_email}']`,
      );
      await issue.first().click({ position: { x: 5, y: 5 } });
      await page.keyboard.press("K");
      await page.getByTestId("sender-kind-dialog").press("p");
      await expectToast(page, "move-sender");
      await expect(issue).toHaveCount(0);
      await page.keyboard.press("u");
      await expect(page.getByText(/is automatic again/).first()).toBeVisible();
      await expect(issue.first()).toBeVisible();
    } finally {
      await bridge(page, "/api/v1/mail/senders/kind", {
        account_id: bundle.account_id,
        sender_email: bundle.sender_email,
        kind: null,
      });
    }
  },
};

/**
 * Verbs in the table the web app no longer offers. Sweep and pin lived on
 * Paper trail and the Reading place: Paper trail became Updates, which lets
 * go per source and per digest, and Reading lets go per edition. The CLI
 * keeps `mxr sweep`, `mxr pin` and `mxr unpin`.
 */
const NO_WEB_SURFACE: Partial<Record<Verb, string>> = {
  sweep: "no web surface since Paper trail and the Reading place left",
  pin: "no web surface since Paper trail and the Reading place left",
};

for (const [verb, entry] of Object.entries(VERB_FEEDBACK) as [
  Verb,
  (typeof VERB_FEEDBACK)[Verb],
][]) {
  const kind = entry.undo === "none" ? "confirms first (irreversible)" : `undoes (${entry.undo})`;
  test(`${verb}: says what it did in the table's words and ${kind}`, async ({ page }) => {
    test.skip(Boolean(NO_WEB_SURFACE[verb]), NO_WEB_SURFACE[verb]);
    const journey = JOURNEYS[verb];
    expect(journey, `${verb} is in the verb table but has no journey here`).toBeDefined();
    if (entry.undo === "none") {
      expect(entry.irreversible && entry.reason && entry.confirm, verb).toBeTruthy();
    }
    await journey!(page);
  });
}

test("toasts sit top centre under the header, clear of search and Compose", async ({ page }) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
  // A toast that stays as it is: star, answered at once with an undo.
  await page.route("**/api/v1/mail/mutations/star", (route) =>
    route.fulfill({
      json: {
        ok: true,
        result: {
          requested: 1,
          succeeded: 1,
          skipped: 0,
          failed: 0,
          mutation_id: "toast-place",
          accounts: [],
        },
      },
    }),
  );
  await page.keyboard.press("s");
  const shown = page.locator("[data-sonner-toast]").first();
  await expect(shown).toBeVisible();
  await expect(shown).toHaveAttribute("data-y-position", "top");
  await expect(shown).toHaveAttribute("data-x-position", "center");
  // A neutral change with an undo is the accent (info) toast.
  await expect(shown).toHaveAttribute("data-type", "info");
  const header = page.locator(".app-shell-topbar");
  const viewport = page.viewportSize()!;
  await expect
    .poll(async () => {
      const [toastBox, headerBox] = await Promise.all([shown.boundingBox(), header.boundingBox()]);
      if (!toastBox || !headerBox) return "not laid out";
      const below = toastBox.y >= headerBox.y + headerBox.height;
      const centre = Math.abs(toastBox.x + toastBox.width / 2 - viewport.width / 2) <= 2;
      return below && centre ? "top centre" : JSON.stringify({ toastBox, headerBox });
    })
    .toBe("top centre");
  // Search and Compose live in the header, so a toast under it covers neither.
  await page.getByRole("button", { name: "Compose" }).first().click({ trial: true });
});
