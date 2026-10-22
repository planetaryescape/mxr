# Keyboard shortcuts: how good systems are designed, and what mxr has today

Research for [chapter 35](../blueprint/35-keyboard.md), 2026-10-22. The external research uses primary sources (help pages, specs, man pages, source at the named commits) where they could be reached. **[unverified]** marks claims seen only in secondary sources or search excerpts. The code audit is at `origin/main` `f14f0b0b3`; none of the key files changed between it and `b3717dbf2`, where most line numbers were read.

## Mail clients

| Client | Grammar | Teaching | Off / remap |
|---|---|---|---|
| Gmail ([help](https://support.google.com/mail/answer/6594)) | Single keys; `g` go and `*` select prefixes; Cmd/Ctrl for compose; Shift opens in a new window | `?` sheet | On/off in settings or `?`; custom shortcuts tab, "one key can refer to only one action" |
| Superhuman | Cmd+K palette over everything; Tab between splits | Palette shows each key; running a command shows its key "for next time" ([blog](https://blog.superhuman.com/how-to-build-a-remarkable-command-palette/)) | No remapping; US QWERTY optimised (help-centre excerpt) |
| HEY | Digits 1–6 for sections; single letters for actions; `;` bulk menu | Each button prints its letter ([37signals](https://updates.37signals.com/post/new-in-hey-power-through-new)) | Not documented **[partly unverified]** |
| Outlook web ([help](https://support.microsoft.com/en-us/office/keyboard-shortcuts-for-outlook-3cdeb221-7ae5-4c1d-8c1d-9e63216c1efd)) | Pick a whole set: Outlook, Gmail, Yahoo, or off | Shift+? shows the active set | Off switch plus preset sets |
| Fastmail ([help](https://www.fastmail.help/hc/en-us/articles/360058753534)) | `g` opens a type-to-jump folder picker; `b` snooze then 1–7 | `?` shows keys for the current screen | One on/off switch |
| Mimestream ([guide](https://mimestream.com/help/user-guide/keyboard-shortcuts)) | Presets: Default, Apple Mail, Gmail; navigation on ⌃⌘digits | — | macOS App Shortcuts |
| Spark ([help](https://support.readdle.com/spark/tips-tricks/use-keyboard-shortcuts)) | Sets: Spark, Mail, Gmail, Custom | — | Record any key per action |
| Shortwave ([docs](https://www.shortwave.com/docs/references/shortcuts/)) | Gmail-like; `g 1–9` favourite searches; Ctrl+K palette | Shift+? | None documented |

Notion Mail closed on 2026-09-22; its page now redirects.

## Terminal mail clients

- **NeoMutt** ([guide](https://neomutt.org/guide/configuration)).
  - Bindings are written per menu (`bind map key function`), with a `generic` fallback and macros that carry descriptions.
  - A short key shadows a prefix: `bind index g …` makes `gg` unreachable until it is unbound.
  - `$help` shows a one-line bar of the menu's main keys.
- **aerc** (`doc/aerc-binds.5.scd`, master).
  - Every binding is keystrokes into an ex command. `:` works everywhere.
  - Contexts are sections (`[messages]`, `[view]`, `[compose::review]`) with account and folder qualifiers.
  - `$noinherit` cuts off the global bindings in a context, and an empty value erases one.
- **Alpine/Pine.** A two-line key menu on every screen; `O` cycles further keys.
- **meli** ([meli.conf.5](https://manpages.opensuse.org/Tumbleweed/meli/meli.conf.5.en.html)). TOML `[shortcuts.<component>]`; `?` is a searchable list; `:` is command mode.
- **Himalaya.** A stateless JSON CLI with separate interactive front-ends. This is the same layering as mxr's CLI under its TUI and web app.

## Apps praised for their keyboards

- **Linear** ([guide](https://linear.app/enablement/guide/navigating-linear)).
  - `G` then a letter for navigation, Cmd+K, and a searchable `?` that was rebuilt "to get more users to adopt shortcuts" ([changelog](https://linear.app/changelog/2021-03-25-keyboard-shortcuts-help)).
  - Tooltips show the keys. Marcin Wichary warns that one place where the feedback is missing makes people give up on it ([post](https://unsung.aresluna.org/linears-visual-key-feedback)).
- **Vim** ([motion.txt](https://vimhelp.org/motion.txt.html)).
  - Operator plus motion, and counts.
  - `timeoutlen` is 1000 ms, and `showcmd` shows the partial command.
- **Helix** ([keymap](https://docs.helix-editor.com/keymap.html)).
  - You select first, then act.
  - Prefixes are minor modes (`g` goto, `z` view, `Space` pickers, `[`/`]` paired motions).
  - `auto-info` lists the next keys.
- **Emacs which-key.** Built into Emacs 30. After one second it shows every binding that can follow.
- **lazygit** ([config](https://github.com/jesseduffield/lazygit/blob/master/docs/Config.md)).
  - Universal keys plus per-panel sections, `?` per panel, and any key can be set to `<disabled>`.
  - `d`, `n`, `e` and `space` keep one verb across panels. `c` means four things, which is the counter-example.
- **VS Code** ([docs](https://code.visualstudio.com/docs/configure/keybindings)).
  - `when` clauses, last rule wins, and `-command` removes a binding.
  - It has a conflicts view and a dispatch troubleshooting log.
- **Zed** ([docs](https://zed.dev/docs/key-bindings)).
  - A context tree where the deeper match wins.
  - Base keymaps, and `null` disables a binding.
  - It waits 1 s between a prefix and a full binding, shows pending keys, and `alt-/` lists candidates.
- **Raycast** ([action panel](https://manual.raycast.com/action-panel)).
  - Enter runs the primary action, Cmd+K opens all actions, and Cmd+Enter runs the second.
  - `Keyboard.Shortcut.Common` keeps muscle memory across extensions ([docs](https://developers.raycast.com/api-reference/keyboard)).
- **Things** ([support](https://culturedcode.com/things/support/articles/2785159/)). Cmd+1–6 for lists, plus "Type Travel": type a list name to jump.

## Platform rules

- **WCAG 2.1 SC 2.1.4, Level A** ([understanding](https://www.w3.org/WAI/WCAG21/Understanding/character-key-shortcuts.html)).
  - A shortcut of printable keys only must be able to be turned off, be remappable to include a modifier, or be active only while its component has focus.
  - `?` and sequences like `g a` count.
  - The reason is speech input.
- **GitHub** ([settings](https://docs.github.com/en/account-and-profile/how-tos/account-settings/managing-accessibility-settings), [Primer](https://primer.style/accessibility/patterns/keyboard-shortcuts)).
  - A "Character keys" off switch; modifier shortcuts keep working.
  - Primer's guidance:
    - scope a shortcut to focus when it only makes sense there;
    - avoid Ctrl/⌘ + W, T, N, F, L;
    - never make a shortcut the only path;
    - give rare or destructive actions no shortcut.
- **Browser-reserved keys.**
  - Chromium's `kReservedCommandIds` covers close tab and window, new tab, window and incognito, restore tab, tab switching and exit. Installed app windows reserve none (`accelerator_table.cc`, `browser_command_controller.cc`, main @ `07ca42e8`).
  - Safari also withholds Cmd+N/W/T/Q/R/L **[secondary]**.
- **`key` versus `code`.**
  - `code` is the physical key, so it cannot name a character on AZERTY ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/code)).
  - VS Code dispatches on `code` or `keyCode`, not `key`, because of dead keys ([wiki](https://github.com/Microsoft/vscode/wiki/Keybinding-Issues)).
  - github/hotkey matches `key`, with US-only shift normalisation.
  - Figma made layout a manual per-user setting after automatic detection failed ([blog](https://www.figma.com/blog/behind-the-scenes-international-keyboard-shortcuts/)).
- **IME.** Ignore keydown when `isComposing || keyCode === 229`. `isComposing` can be false on the edge keystrokes ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Element/keydown_event)).
- **macOS HIG** ([keyboards](https://developer.apple.com/design/human-interface-guidelines/keyboards)).
  - Custom shortcuts are only for the most frequent commands, and standard ones (⌘W, ⌘N, ⌘Q, ⌘,, ⌘?) are never repurposed.
  - Shift complements a related shortcut, and you never add a modifier for an unrelated command.
  - Modifiers are listed as Control, Option, Shift, Command.
  - Avoid characters that need Option on some layouts.
  - Menu commands can be rebound by users through App Shortcuts.
- **Terminals** ([kitty protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/), [tmux](https://man.openbsd.org/tmux.1), [crossterm](https://docs.rs/crossterm/latest/crossterm/event/struct.PushKeyboardEnhancementFlags.html)).
  - In legacy encoding, Ctrl+I/M/H/[ are Tab, Enter, Backspace and Esc. Ctrl+Shift+letter equals Ctrl+letter. Alt arrives as an Esc prefix.
  - Ctrl+S/Q are flow control. tmux's prefix is C-b and screen's is C-a.
  - The kitty protocol fixes this progressively. crossterm 0.29 exposes the flags through `PushKeyboardEnhancementFlags` and `supports_keyboard_enhancement()`.
  - tmux is not on kitty's list of implementers.
- **Prefix timing.**

  | System | Timeout | Pending state shown |
  |---|---|---|
  | github/hotkey (`src/sequence.ts`, `da336037`) | 1500 ms | No |
  | Vim | 1000 ms | `showcmd` |
  | Zed | 1000 ms | Status bar |
  | which-key | Pops up after 1.0 s | The popup |
  | Gmail, Linear | Undocumented **[unverified]** | Undocumented |

## Principles that recur

| Principle | Sources |
|---|---|
| Teach the key where the command is used: palette, tooltip, button, per-screen help | Superhuman, Linear, HEY, Fastmail, lazygit, Alpine, NN/g ([UI accelerators](https://www.nngroup.com/articles/ui-accelerators/)) |
| Every action has a path without a key; every key has a palette entry | Superhuman, Primer, NN/g |
| Layered grammar: single keys for frequent verbs, one prefix per family, modifiers for app commands, palette for the rest; a prefix never acts alone | Gmail, Linear, neomutt, github/hotkey |
| Same key, same verb, in every context | lazygit (and its `c` counter-example), Raycast |
| Direct keys for a small fixed set of places, a type-to-jump picker for open sets | HEY, Things, Mimestream, Fastmail, k9s |
| Explicit contexts with defined precedence, and a way to debug them | VS Code, Zed, aerc, neomutt, lazygit |
| Show the pending prefix and what can follow, about 1 s timeout | Helix, which-key, Zed, Vim |
| Single-character keys can be turned off or remapped and never fire while typing | WCAG 2.1.4, GitHub, Outlook, Fastmail, MDN |
| Respect reserved keys per platform | Chromium, Safari, Apple HIG, kitty, tmux |
| Plan for non-US layouts: prefer letters, match on `key`, let people remap | Figma, VS Code, Apple HIG, Superhuman and Notion Mail admitting QWERTY-only |
| Keep the set small; the palette carries the long tail | Apple HIG, Primer, aerc, HEY |

## mxr today

### TUI

- **Dispatch is hand-written.** It runs through `match` arms in about 25 handlers:
  - `crates/tui/src/input.rs:231-565` for motions and prefixes;
  - `crates/tui/src/app/input.rs:279-336` for mail verbs;
  - `app/input.rs:356-1662` for the modal stack, 38 overlays deep;
  - per-mode handlers at `app/now_actions.rs:381`, `messages_actions.rs:616`, `todo_actions.rs:609`, `updates_actions.rs:424`, `reading_actions.rs:662` and `records_actions.rs:780`;
  - per-screen handlers at `app/input.rs:1743-2739`.
- **The keymap tables are for display only.** `keybindings.rs:663-848` feeds help, the hint bar, the palette and the retirement of remapped defaults. Without a `keys.toml` the configured path returns `None` straight away (`input.rs:140-145`).
- **The modes have no context of their own.** All six are `UiContext::MailboxList` (`app/screen_helpers.rs:4-39`), so the hint bar and help describe a mailbox (`ui/hint_bar.rs:157-166`).
- **Prefix chords bypass the mode filters** (`app/input.rs:1494-1505`).
- **Help is mostly hand-written** (`ui/help_modal.rs:94-531`). The mode sections come from `crates/protocol/src/types/mode_guide.rs`, which names keys as literals.
- **Remapping has been honoured only since #528** (`bcee881b3`, 2026-10-20). Retiring `e` in `[mail_list]` also swallows `e` in Now (`keybindings.rs:613-628`, `app/input.rs:1480-1490`).
- **Terminal defects:**
  - no bracketed paste or keyboard enhancement (`runner.rs:446`);
  - `q` quits from any pane (`input.rs:511`);
  - `Ctrl-a` is overloaded (`assistant_actions.rs:63`, `app/input.rs:844`).
- **Destructive keys:**
  - `Esc` on the send confirm discards the draft (`app/input.rs:970-979`);
  - `#` deletes a non-category rule with no preview (`app/rule_actions.rs:49-68`).

### Web

- **The registry checks clashes when actions are defined.** Duplicate ids, a duplicate chord in one scope and a prefix that is also a full binding are all rejected (`apps/web/src/lib/actions/registry.ts:29-58`).
- **There is one dispatcher** (`lib/keys/dispatcher.ts:47-173`). It waits 1000 ms after a prefix. On a dead end it runs the second key alone (`:153-164`).
- **The `place` scope is never pushed**, yet its actions are still listed (`features/places/actions.ts:21-79`).
- **The docs generator skips the modes** (`keybindingsDoc.ts:13-55`). It omits 87 bound actions.
- **About 40 handlers sit outside the registry:**
  - compose chords (`features/compose/session/composeShortcuts.ts:33-47`);
  - dialog letters (`LinksDialog`, `UnsubscribeDialog`, `SnoozeDialog`);
  - mode pickers (`MoveToModeDialog`, `SenderKindDialog`);
  - arrival rows (`ArrivalsRoute.tsx:80-102`);
  - assistant cards (`Proof.tsx`, `Question.tsx`, `History.tsx`).
- **The two pickers letter the same destinations differently** (`arrivals/types.ts:120-126` against `places/placeCopy.ts:31-42`).
- **Characters typed with Alt or Option get `Alt` added** (`lib/keys/chord.ts:55`), so `#` on a UK Mac and `[ ] { }` on AltGr layouts never match.
- **IME is guarded in four places only.** Nine Enter-to-submit fields have no guard.
- **No off switch or remapping**, which fails WCAG 2.1.4.
- **The reserved-key list exists twice.** `dispatcher.ts:175-184` and `browserReserved.ts:24-56` disagree. Compose's `Mod+s` and `Mod+Shift+R` escape the guard.
- **Focus returns to a pane only where one is marked.** Only the list, reader and sidebar set `data-active-pane`, so focus ends up on `<body>` after an overlay closes in the modes.

### Between the clients

- **Two definitions compared by a probe.** `docs/reference/tui-keymap.json` is probed from the TUI (`crates/tui/src/runner/tests/keymap.rs:1005-1021`). `apps/web/src/lib/actions/keymapParity.ts` compares it with the web registry through `SAME_ACTION` and about 60 `KEYMAP_DIFFERENCES` (`:273-649`).
- **The comparison is narrow.** It covers 11 contexts and none of the overlays, compose or the assistant.
- **Keys the probe misses still differ.** Examples:
  - assistant history delete is `d` on the web and `D` in the TUI;
  - snooze presets are digits on the web and `j`/`k` in the TUI;
  - a newline in the assistant is `Shift+Enter` on the web and `Alt-Enter`/`Ctrl-j` in the TUI.

### Churn

- **157 commits** have touched the key files: the TUI keymap and dispatch, help, hint bar, palette, `tui-keymap.json`, `apps/web/src/lib/keys`, `apps/web/src/lib/actions` and the site keybindings page. 51 of them came after 2026-09-27.
- **Advertised keys that did nothing were fixed five times:** `8c0e170f4`, `be2daa5eb`, #240, #252 and #528.
- **Keys mostly arrived inside mode PRs**, each adding its own handler. Examples:
  - place (`4abc93c53`);
  - To do and Now (`89757173a`, `ad78800f8`);
  - Messages and Archive (`584ed33b8`, `260a2991c`);
  - Reading and Updates (`a95d86a53`, `f72f89cc1`);
  - the assistant (`1e326a210`).

## Open research

- **Tauri key handling.** Does the Tauri window (WKWebView) deliver `Mod+1`–`9`, `Mod+T` and `Mod+L` to the page? This is spike S-K1 in chapter 35.
- **Gmail's and Linear's prefix timeouts** need a live test.
- **Superhuman's help centre** returned 403 to every fetch, so its claims beyond the blog rest on search excerpts.
