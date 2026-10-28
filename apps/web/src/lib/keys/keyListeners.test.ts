/* @vitest-environment node */

/*
 * The escape hatch from the key definition (blueprint 35, "Both clients
 * dispatch by action id, never by key"). Every page-level key lives in
 * `crates/keymap/keymap.toml` and reaches the page through the dispatcher,
 * so help, the docs and the reserved-key check see it. Raw key handling is
 * allowed only in the files below, each for one of the rule's reasons. A
 * new listener fails here until it moves into the definition or earns a
 * place on the list.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

import { describe, expect, test } from "vitest";

const SRC = join(__dirname, "../..");

/** Raw key handling: DOM listeners, React handlers, and editor keymaps. */
const RAW_KEYS =
  /addEventListener\(\s*["']keydown["']|onKeyDown(Capture)?=|keymap\.of\(|handleKeyDown\s*:|domEventHandlers|tinykeys\(/;

type Rule =
  | "text field or editor"
  | "overlay with its own letters"
  | "arrow-key widget"
  | "dispatcher or observer"
  | "compose"
  | "page shortcut not yet moved";

/**
 * Paths under `src`. "page shortcut not yet moved" is debt, not a rule: each
 * such entry says why it hasn't moved yet, and no new one joins it.
 */
const ALLOWED = {
  // Text fields and editors: keys there are typing.
  "features/archive/ArchiveRoute.tsx": {
    rule: "text field or editor",
    why: "Esc clears or leaves the ask field",
  },
  "features/ask/AskArchivePanel.tsx": {
    rule: "text field or editor",
    why: "Enter asks, Shift+Enter adds a line",
  },
  "features/assistant/Composer.tsx": {
    rule: "text field or editor",
    why: "Enter sends the assistant message; Esc hands keys to a waiting card",
  },
  "features/compose/ComposeLauncher.tsx": {
    rule: "text field or editor",
    why: "the recipient field's Tab and → accept the suggestion",
  },
  "features/compose/DraftHelpLine.tsx": {
    rule: "text field or editor",
    why: "Enter submits the instruction, Esc closes the line",
  },
  "features/compose/DraftProof.tsx": {
    rule: "overlay with its own letters",
    why: "the proof's keys while it is open: Enter accepts, Esc keeps mine, ← → steps versions, Tab moves gap to gap",
  },
  "features/compose/RecipientField.tsx": {
    rule: "text field or editor",
    why: "recipient chips: commas, Backspace and suggestion keys",
  },
  "features/compose/codemirror/CodeMirrorComposeEditor.tsx": {
    rule: "text field or editor",
    why: "the CodeMirror body; its send and save chords come from the definition",
  },
  "features/compose/tiptap/TiptapComposeEditor.tsx": {
    rule: "text field or editor",
    why: "the rich-text body; its compose chords come from the definition",
  },
  "features/mailbox/ListWithReader.tsx": {
    rule: "text field or editor",
    why: "the list filter field's Esc and Mod+Enter",
  },
  "features/reading/HighlightToolbar.tsx": {
    rule: "text field or editor",
    why: "the highlight note field",
  },
  "features/search/SavedSearchManager.tsx": {
    rule: "text field or editor",
    why: "Esc cancels the saved-search form",
  },
  "features/search/SearchToolbar.tsx": {
    rule: "text field or editor",
    why: "Esc leaves the search query field",
  },
  "features/time/NaturalTimeInput.tsx": {
    rule: "text field or editor",
    why: "the time picker's field and its which-time-did-you-mean choices",
  },

  // Overlays that print their option letters, or close on Esc.
  "components/RightRail.tsx": {
    rule: "overlay with its own letters",
    why: "Esc closes the rail before it reaches the conversation underneath",
  },
  "features/arrivals/MoveToModeDialog.tsx": {
    rule: "overlay with its own letters",
    why: "the move picker's destination letters",
  },
  "features/arrivals/NotSureSection.tsx": {
    rule: "overlay with its own letters",
    why: "a not-sure card opens the X picker, says its old letters moved, and its y/n ask, while focused",
  },
  "features/assistant/AssistantSplit.tsx": {
    rule: "overlay with its own letters",
    why: "Esc closes the assistant drawer",
  },
  "features/assistant/History.tsx": {
    rule: "overlay with its own letters",
    why: "the assistant's past-conversations list",
  },
  "features/assistant/Proof.tsx": {
    rule: "overlay with its own letters",
    why: "the assistant's proof card: y, n, x",
  },
  "features/assistant/Question.tsx": {
    rule: "overlay with its own letters",
    why: "the assistant's question card: numbered options",
  },
  "features/assistant/usePreviewOpen.ts": {
    rule: "overlay with its own letters",
    why: "Esc dismisses the hover preview",
  },
  "features/compose/SendConfirmDialog.tsx": {
    rule: "overlay with its own letters",
    why: "the send confirmation's Enter and override letter",
  },
  "features/mail-actions/dialogs/CategoriesDialog.tsx": {
    rule: "overlay with its own letters",
    why: "lets a row's manage button take Enter from cmdk",
  },
  "features/mail-actions/dialogs/LabelsDialog.tsx": {
    rule: "overlay with its own letters",
    why: "Mod+Enter applies the picked labels",
  },
  "features/mail-actions/dialogs/LinksDialog.tsx": {
    rule: "overlay with its own letters",
    why: "the links picker's j, k, Enter and o",
  },
  "features/mail-actions/dialogs/MoveDialog.tsx": {
    rule: "overlay with its own letters",
    why: "Enter confirms the route preview",
  },
  "features/mail-actions/dialogs/ReplyLaterDialog.tsx": {
    rule: "overlay with its own letters",
    why: "Enter with no time is a plain reply later (the palette's Reply later…)",
  },
  "features/mail-actions/dialogs/NotNowDialog.tsx": {
    rule: "overlay with its own letters",
    why: "digits pick the numbered presets, the arrows move the highlight",
  },
  "features/mail-actions/dialogs/UnsubscribeDialog.tsx": {
    rule: "overlay with its own letters",
    why: "u and a pick the printed unsubscribe options",
  },
  "features/messages/ConversationPeople.tsx": {
    rule: "overlay with its own letters",
    why: "Escape closes the scoped people disclosure and restores trigger focus",
  },
  "features/messages/TopicSwitcher.tsx": {
    rule: "overlay with its own letters",
    why: "Escape closes the topic disclosure and restores trigger focus",
  },
  "features/places/SenderKindDialog.tsx": {
    rule: "overlay with its own letters",
    why: "the sender-kind picker's letters",
  },
  "features/reading/ReadingDialogs.tsx": {
    rule: "overlay with its own letters",
    why: "u and a pick the printed unsubscribe options",
  },
  "features/search/SearchPalette.tsx": {
    rule: "overlay with its own letters",
    why: "the search palette's arrows and Ctrl+j/k move through results",
  },

  // Arrow-key widgets.
  "components/Segmented.tsx": {
    rule: "arrow-key widget",
    why: "a segmented control's arrows move the choice",
  },
  "components/Page.tsx": {
    rule: "arrow-key widget",
    why: "tab groups own arrows, Home and End while a tab has focus",
  },
  "components/PageFrame.tsx": {
    rule: "arrow-key widget",
    why: "link-based section headers move focus with arrows, Home and End; Enter remains native",
  },
  "features/analytics/WrappedDashboard.tsx": {
    rule: "arrow-key widget",
    why: "the wrapped dashboard's arrows step the cards",
  },
  "features/onboarding/OnboardingRoute.tsx": {
    rule: "arrow-key widget",
    why: "the provider tiles' arrows, Home and End",
  },

  // The dispatcher and what watches it.
  "lib/keys/dispatcher.ts": { rule: "dispatcher or observer", why: "the one key dispatcher" },
  "lib/keys/typeAhead.ts": {
    rule: "dispatcher or observer",
    why: "holds keys typed before a field takes focus",
  },
  "lib/actions/keyHints.ts": {
    rule: "dispatcher or observer",
    why: "counts key presses as keyboard use",
  },
  "features/sound/player.ts": {
    rule: "dispatcher or observer",
    why: "unlocks audio on the first key press",
  },
  "features/hints/hintSlot.ts": {
    rule: "dispatcher or observer",
    why: "counts a key press as a use of the hint slot",
  },
  "features/thread/MessageBody.tsx": {
    rule: "dispatcher or observer",
    why: "forwards keys typed in the message frame to the dispatcher",
  },

  // Compose owns the keyboard and runs its definition chords itself
  // (`composeShortcuts.ts`; the dispatcher stands aside while it is open).
  "features/compose/ComposeEditorPanel.tsx": {
    rule: "compose",
    why: "runs the compose chords, by id, and the discard dialog's Enter",
  },
  "features/compose/ComposeHost.tsx": {
    rule: "compose",
    why: "runs compose.fullscreen, by id",
  },

  "features/focus/FocusRoute.tsx": {
    rule: "page shortcut not yet moved",
    why: "Tab skips only from the page itself; the dispatcher can't tell that from Tab on a button",
  },
  "features/places/PlaceLayout.tsx": {
    rule: "page shortcut not yet moved",
    why: "Esc closes the conversation when no scope took it; each mode's Esc differs (K2's q/Esc layering)",
  },
} satisfies Record<string, { rule: Rule; why: string }>;

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === "node_modules" ? [] : sourceFiles(path);
    // Declaration files hold types, never a listener.
    if (!/\.tsx?$/.test(entry.name) || /\.(test|d)\.tsx?$/.test(entry.name)) return [];
    return [path];
  });
}

const withRawKeys = sourceFiles(SRC)
  .filter((path) => RAW_KEYS.test(readFileSync(path, "utf8")))
  .map((path) => relative(SRC, path).split("\\").join("/"));

describe("key listeners outside the definition", () => {
  test("live only in the allowed files", () => {
    // A file here should move its key into crates/keymap/keymap.toml and
    // dispatch it by action id, or join ALLOWED with its reason.
    const unexplained = withRawKeys.filter((path) => !Object.hasOwn(ALLOWED, path));
    expect(unexplained).toEqual([]);
  });

  test("every allowed file still handles keys, so the list can't rot", () => {
    const stale = Object.keys(ALLOWED).filter((path) => !withRawKeys.includes(path));
    expect(stale).toEqual([]);
  });

  test("the pattern sees each kind of listener", () => {
    const samples = [
      'window.addEventListener("keydown", onKeyDown)',
      "<div onKeyDown={handle} />",
      "<div onKeyDownCapture={handle} />",
      "keymap.of([{ key: 'Mod-s' }])",
      "editorProps: { handleKeyDown: (view, event) => false }",
      "EditorView.domEventHandlers({ keydown })",
    ];
    expect(samples.filter((sample) => !RAW_KEYS.test(sample))).toEqual([]);
    expect(RAW_KEYS.test('addEventListener("pointerdown", unlock)')).toBe(false);
  });
});
