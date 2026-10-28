# 35. Keyboard

mxr is keyboard-native: "everything is fully navigable by keyboard, including anything you want to do" (BK, 2026-05-11, D130). Every action has a key or a palette entry, and every key a person learns has to keep meaning the same thing. This chapter sets out how the keys work as one system:

- the grammar that decides which key an action gets;
- the contexts that decide which binding is live;
- the single definition that the TUI, the web app, help, the palette and the docs all read;
- the rules that keep keys portable and accessible;
- the target key map;
- the order in which today's keys move to it.

Status: K1a, K1b and most of K1c built: `crates/keymap`, `GetKeymap`, `mxr keys` and `mxr keys check`, the web registry and every TUI mode reading the definition, shared status strips, and per-mode `keys.toml` sections. K2 is built, in the web app (#652) and the TUI (#656). K3's first three waves of key moves are built: the mode verbs, then the `g` table, then the rest of the verb table's single keys, each old key naming its new one until 0.6.180 (the third wave's until 0.6.185). Wave 4a, the pickers, is built in both clients: `X` is one destination picker whose capitals move the sender and which answers Not-sure and new-sender questions (`T` and the new-sender digits moved into it), the sender-kind picker uses the `g` letters, `b` is one Not-now picker with Later first and the presets on digits (`Z` moved into it), whois moved to the person page and the palette, and the pickers it touched follow the overlay grammar; those old keys name their new ones until 0.6.197. `Enter` as "not here" in the `X` picker is not built: the daemon has no request for it outside To do and Archive. Wave 4b, teaching, is built too: the digits pick To do's and Reading's tabs (Reading's Highlights tab is the open item's highlights; no page of every highlight exists yet), "Next time" names a key once per action and profile in both clients, and the TUI palette is built from the definition with every key; the old keys say where they went until 0.6.197. The TUI's help and the keybindings page's TUI tables are generated from the definition, which now also lists the tool screens', Search's and the assistant pane's keys (a TUI test holds each list to its handler). K4's first half is built: `keys.toml`'s `single_keys` switch and `[actions]` keys apply in both clients, served by `GetKeymap` and written by `UpdateKeys` (with a dry run) from `mxr keys set`, `reset` and `single-keys` and the web app's Settings → Keyboard editor, while the mode and pane sections stay TUI-only. K3's parity and overlay work is built: `cargo test -p mxr-keymap` compares the clients from the definition alone (`Keymap::client_differences`, no exceptions file; `keymapParity.ts` is gone), the TUI took the web's `g #`, `g !`, `g w`, `g f`, motion keys, `Delete`, reader `n` and `N` and the sidebar's own first, last and undo, the web took Reading's `t`, both screeners decide with letters, and overlays close on `q` and take `y` and `n` as the grammar says; the old keys say where they went until 0.6.200. K4's second half is built: after 300 ms a prefix lists its next keys in both clients, the TUI from the same rows as its help, both clients wait four seconds for the next key, and the desktop app's menu is built from the definition and runs the web app's actions by id, with accelerators only for actions already on a modifier chord. The K3 follow-ups are built: every overlay's keys are in the definition, so help and the keybindings page generate them in both clients, a TUI test holds each overlay's handler to its entries and the web's dialogs read theirs by id; the web takes list and reader keys from the panes beside them and `h` in the modes, as the TUI does, so no `[[tui_pass_through]]` entry is left; the unsaved-account-form prompt names "Discard draft", the send review's `Enter` runs its lit option, and Unsubscribe's archive option is `e` in both clients (`a` says so until 0.6.204). The TUI has Snoozed (`g n`), Wake (`w`) in Snoozed and the reply queue, and the Updates peek (`Space`), so none of them is web-only now; the web desk's Done left `w` for `e` (`w` says so until 0.6.206). The TUI has the Focus & reply page on `g F`, with the web's queue, keys and requests, so the check now compares the focus keys too; its Send-and-remind is palette-only because `w` is Wake (`w` says so until 0.6.206), and `d` earned its key as Draft in your voice. The rest is not built. Written on 2026-10-22 against `origin/main` at `f14f0b0b3` (v0.6.142). It replaces A005's key map and chapter 08's "Default keybindings", "Multi-key sequences", "Configurable keybindings" and "Action dispatch system". It refines chapter 22's "One key map across Now and the modes", whose verbs it keeps (D163 to D173). The research behind it is in [keyboard-shortcuts.md](../research/keyboard-shortcuts.md).

## Today's keys record collisions, not a design

A005 ordered the sources as "vim first, Gmail second, custom last", but it gave no rule for the custom keys. Each feature PR since then took whichever letter was free in its own handler, and each collision was settled locally: chapter 22's key table spends a page moving Archive's research keys off `c`, `f`, `x`, `v`, `d` and `e`. The result is a key map that grew by accretion.

### One letter means up to eight things

Counted across both clients at `f14f0b0b3`:

| Key | What it does today |
|---|---|
| `e` | Archive (list); done (Now, Messages, To do); let go (Reading); let go of the source (Updates); **open the email** (Archive); toggle a rule (Rules); edit (drafts, saved searches, highlights); expand steps (assistant) |
| `p` | Sender profile (list); pin (Updates, place); person page (Messages); issuer page (Archive); push a draft (drafts); Paper trail (screener) |
| `A` | Attachments (list); sweep the place; let go of all (Now, Updates, Reading, catch-up); apply a treatment (Rules); confirm recovery (Diagnostics) |
| `o` | Open (list); open in the browser (TUI reader); toggle one message (web reader); show as sent (Messages); open the email (modes); override send (web send confirm) |
| `K` | Previous message (reader); sender menu (list, Now, Messages); sender kind (place, Reading); tune (Updates) |
| `L` | Open links (list, reader); vim viewport bottom (modes, sidebar); open the link (Updates); fetch the article (Reading); history (assistant); logs (Diagnostics) |
| `Esc` | Back; dismiss a hint; close; nothing (Reading list); **discard the draft** (TUI send confirm, `crates/tui/src/app/input.rs:970-979`) |

The reverse problem is just as common: one verb sits on different keys in different places. In Archive, the email opens with `o` or `e`, and in Messages with `o` or `v`. Pin is `s` in Messages and `p` in Updates. Delete is `#`, `d` or `D` depending on the screen. Create is `c` in mail and `n` on the Rules, Accounts and saved-search screens. Done is `e`, `Space` (web To do) or `w` (web list).

Some overloading is deliberate and good. Chapter 22's `e` means "done here", and each mode decides what done means: archive, tick off, let go. That is the model the rest of the map should follow. The collisions are the keys where a mode borrowed the letter for an unrelated verb.

### The key map is not what dispatches keys

In the TUI, every keypress goes through hand-written `match` arms spread over about 25 handlers:

- `crates/tui/src/input.rs:231-565` and `crates/tui/src/app/input.rs:279-336`;
- one `*_lens_key` function per mode, in `app/{now,messages,todo,updates,reading,records}_actions.rs`;
- one per screen, in `app/input.rs:1743-2739`.

The keymap tables in `crates/tui/src/keybindings.rs:663-848` only feed help, the hint bar and the palette, and tell the remap layer which defaults to retire. The audit and #528 found that `keys.toml` was loaded by nothing for seven months while the docs advertised it.

The TUI has no context for the modes. All six report as `UiContext::MailboxList` (`app/screen_helpers.rs:4-39`), so the hint bar in Now shows `e:Archive` while `e` is Done (`ui/hint_bar.rs:157-166`). Help lists `# Trash` in modes that filter trash out. Prefix keys bypass the mode filters, because a pending chord's second key runs before any mode handler (`app/input.rs:1494-1505`), so `i a` still accepts an invitation in To do.

The web app has a real registry (`apps/web/src/lib/actions/registry.ts`) with clash checks, one dispatcher and generated help, but:

- About 40 handlers sit outside the registry: compose chords, dialog letters, mode pickers, arrival rows and the assistant. Help, the palette, the docs and the reserved-key guard cannot see them.
- The `place` scope is never pushed, yet its ten bindings still appear in Help and the docs and pass the parity test (`features/places/actions.ts:21-79`).
- The generated docs drop 87 bound actions, because `keybindingsDoc.ts` has no section for the modes.

The two clients share no definition. `docs/reference/tui-keymap.json` is probed from the TUI dispatcher, and `keymapParity.test.ts` compares it with the web registry through a hand-kept table of about 60 documented differences (`apps/web/src/lib/actions/keymapParity.ts:273-649`). The comparison covers 11 contexts. It does not cover pickers, confirms, compose, the assistant or any key the web registry cannot see.

### Overlays have no shared grammar

| Part | Variants today |
|---|---|
| Close | `Esc`; `Esc` or `q`; `Esc`, `q`, `x` or `Enter`; or the key that opened the overlay |
| Confirm | `y` or `Enter`; `y` only; `Enter` or `u`; `s`; `Enter` twice; `Enter`, but `y` only for one sender |
| Cancel | `Esc` or `n`, sometimes `q` |

Two pickers that sit next to each other letter the same destinations differently:
- the move-to-mode picker uses `m x u r e` (`arrivals/types.ts:120-126`);
- the sender-kind picker uses `p r t x a` (`places/placeCopy.ts:31-42`);
- the screener and new-sender digits put Paper trail and Block on opposite numbers.

Some destructive keys skip the rule that destructive actions preview first:
- `#` on Rules deletes a rule with no preview (`app/rule_actions.rs:61-68`);
- `d` in the web assistant history deletes a conversation with no confirm (`features/assistant/History.tsx:66-69`);
- `Esc` on the TUI send confirm throws the draft away.

### Platform rules are broken in ways people will hit

- **No way to turn single-character shortcuts off.** The web app offers neither an off switch nor remapping, and its keys act wherever focus is, outside fields. That fails WCAG 2.1.4 (Level A), whose stated reason is speech-input users, whose dictation arrives as letters.
- **Characters typed with Option or AltGr miss their binding.** `tokenFromEvent` adds `Alt` to them (`apps/web/src/lib/keys/chord.ts:55`), so `#` on a UK Mac (Option-3) never trashes, and `[`, `]`, `{` and `}` are unreachable on German and French layouts.
- **IME composition is half-guarded.** Nine Enter-to-submit fields skip the guard. Even where it exists, the guard checks `isComposing` but not `keyCode === 229`, which MDN says is needed on the edge keystrokes.
- **Pasted text acts as keystrokes in the TUI.** Bracketed paste is off, so pasting into the list fires `#`, `!` and `e`.
- **Documented terminal keys can never arrive.** Without the kitty protocol, `Ctrl-Shift-A` reaches the TUI as `Ctrl-a`, and `Ctrl-a` is GNU screen's prefix. The config docs still give `Ctrl-Shift-A` as an example.
- **`q` quits from anywhere.** In the TUI, one `q` too many after closing a modal exits mxr.

### The churn is the cost

Since #233 rebuilt the web dispatcher on 2026-09-27, 51 commits have touched the key files. That is 157 in all, counting the TUI keymap, help, palette, parity and site docs. Advertised-but-dead keys were fixed five separate times: `8c0e170f4`, `be2daa5eb`, #240, #252 and #528. The send-confirm override took two releases to match across clients (#518 web, #577 TUI). Each new mode PR regenerated the parity table rather than fitting a design.

### What already works stays

- The Gmail core and the `g` and `*` prefixes.
- The web registry's definition-time clash checks.
- The generated TUI keymap JSON and the parity idea.
- Chapter 22's "`e` is done here" verb.
- D130's rule that keys show on the lit row, in the status strip, in `?` and in the palette, and nowhere else.

## Ten rules decide every key

These rules come from the systems that people praise for their keyboards. Each rule cites its sources in the research note.

1. **A key names a verb, and a verb has one key everywhere.** A mode decides what the verb means there, as `e` already does. A context may not reuse a verb's key for a different verb. lazygit's `d`, `n`, `e` and `Space` show the rule working across panels. Its `c`, which means four different things, shows the cost of breaking it. Raycast ships `Keyboard.Shortcut.Common` for exactly this reason.
2. **A single key is earned by frequency.** The palette carries the long tail. Apple's HIG reserves custom shortcuts for "only the most frequently used app-specific commands". Primer says rare and destructive actions get none. A new verb takes a single key only when it is part of a daily flow, and it is visible on the lit row while that is being proved.
3. **Gmail and vim keys are frozen.** Our verbs fill the gaps without renaming theirs. Outlook, Mimestream, Spark and Shortwave all ship Gmail compatibility because those keys are portable muscle memory. A key moves only when it breaks one of these rules. A key that is merely hard to remember stays where it is.
4. **Shift means the same verb, further.** It acts on all of the view instead of this item, on the sender instead of the email, or outside mxr instead of inside. Shift on a reader letter shows another part of the email. Any other capital is either a frozen Gmail, vim or A005 key, or one of two named mxr verbs with no lowercase form, `X` and `B`. Apple's HIG: Shift "complements a related shortcut", and you don't add a modifier to an existing key for an unrelated command.
5. **Places live behind `g`, selections behind `*`, invitations behind `i`.** Nothing else gets a prefix. A key that starts a prefix never acts alone (neomutt, github/hotkey).
6. **Contexts form a tree with fixed precedence, and a few keys are reserved.** `?`, `/`, `:`, `g`, `Esc` and `u` keep their verb in every context. A context may specialise one (Archive's `/` searches by asking) but never give it another verb, and the definition fails to load if one tries (VS Code `when` clauses, Zed's context tree, aerc sections).
7. **Every overlay closes, confirms and cancels the same way.** See the overlay grammar below.
8. **Every client uses the same key for the same action, from one definition.** The TUI, the web app, the desktop app and the Raycast extension read one definition. It drives dispatch, help, hints, the palette, the docs and remapping. A client differs only where it cannot deliver the key, or where it lacks the feature, and the definition records which (see "Every client uses the same keys"). A key that is not in the definition does not exist.
9. **Every key is visible where it is used, and every action works without one.** This is D130, plus the palette showing every key and a hint for keys that moved (Superhuman, Linear, NN/g, Primer).
10. **Keys are accessible and portable by default.**
    - Single-character keys can be turned off or remapped.
    - Keys never fire while you type.
    - Symbol keys always have a letter or palette path.
    - Terminal defaults avoid keys a terminal cannot deliver.
    - Destructive keys preview or undo.

## The grammar has five layers

### Web choice controls and discovery (26 October 2026)

Keyboard discoverability is part of the interaction, not a trip to the manual.
Pausing after a prefix shows its live continuations from the action registry,
with scope precedence and unavailable actions respected. Fast chords do not
wait. The web sequence timeout is four seconds so the guide can be read;
Escape, pointer interaction, text-field focus and window blur cancel it.

`g f` focuses filters in the mail views, and the selected header tab or first
filter on tool and settings pages. Show the key beside those controls.
Search's filter shortcut must move focus into the revealed controls, not
leave mail-list actions active behind them.

Tab and segmented-choice widgets share wrapping arrow-key navigation and
Home/End. Only the selected tab or segment is in the Tab order. Header links
keep native link semantics: arrows move focus; Enter opens the link. Existing
reader view and Reading issue/article shortcuts are printed beside their choices.

Messages additionally earns `f` for its frequent four-view cycle, with an
in-place hint and unchanged Unread/Past your pace toggles. This is local to
Messages; mail-list and email-reader `f` remains Forward. Do not impose a
cycle key on surfaces where it would replace an established mail verb.

| Layer | What goes there | Examples |
|---|---|---|
| Single keys | The frozen Gmail and vim set, plus the mxr verbs in the table below | `j` `k` `e` `r` `t` `X` |
| Shift | The same verb, further, by rule 4 | `e` done, `E` done for all in view |
| Prefixes | `g` go, `*` select, `i` invitation | `g x` To do, `* u` unread |
| Modifier chords | App-level and writing commands: palette, send, compose formatting | `Mod+K`, `Mod+Enter` |
| Palette | Everything, with its key if it has one | Export, Whois, Microsoft To Do sync |

`Mod` is Cmd on macOS and Ctrl elsewhere. In the TUI it is Ctrl, and terminals limit the safe set (see "Terminal and browser keys").

### The verb table

This is the whole single-key namespace for mail, Now and the five modes. Source G is Gmail, V is vim, A is A005, and 22 is chapter 22. A dash in "Moves from" means the key keeps its meaning.

| Key | Verb | What it means per context | Source | Moves from |
|---|---|---|---|---|
| `j` / `k` | Next / previous | Row in lists, line in the reader | V, G | — |
| `J` / `K` | Next / previous message in a thread | Reader | V (Gmail `n`/`p`) | `K` sender menu moves to `P` |
| `g g` / `G` | First / last | Every list, the reader, and now the modes too | V | — |
| `Ctrl-d` / `Ctrl-u` | Half page down / up | Lists, the reader, the modes | V | — |
| `Enter` | The primary verb | Open; open the person; do it; read; open the document | 22 | — |
| `Space` | Peek, or page down | Peek without opening in lists (macOS Quick Look); page down in the reader | browser, macOS | web To do done moves to `e` |
| `h` / `Esc` / `q` | Back | See "Back and quit" | V | — |
| `o` | Open the email itself | List: open the row. Reader: expand or collapse the focused message (Gmail). Modes: the email behind the item | G, 22 | `e` and `v` aliases retire; TUI reader `o` moves to `O` |
| `O` | Open outside mxr | The browser or original HTML; Reading opens in Marginalia (D121) | rule 4 | Reading `M` |
| `e` | Done here | Archive (mail); tick off (To do); let go (Reading, Updates source); done (Now, Messages, Focus) | G, 22 | Archive `e` (open email) moves to `o` |
| `E` | Done for everything in view | With a dry-run preview first | rule 4 | `A` let go of all, `S` sweep; export moves to the palette |
| `[` / `]` | Done here, then previous / next | Reader | G | — |
| `{` / `}` | Previous / next group | Topic (Messages), year (Archive), section (sidebar, where `Enter` on a section header collapses or expands it) | V paragraph | `[` / `]` in Messages and Archive; sidebar `[` / `]` collapse and expand |
| `n` / `N` | Next / previous item from the reader | The next result in the list you came from | V | — |
| `r` / `a` / `f` | Reply / reply all / forward | Mail, Now, Messages | G | — |
| `c` | Create what this place holds | Compose (mail); new topic with this person (Messages); new rule, account, saved search, category (their screens) | G, Linear | `n` new on the Rules, Accounts and sidebar screens |
| `#` | Delete, with undo or preview | Trash (mail); delete a rule, draft, saved search or conversation | G | `d` and `D` deletes; Rules `#` gains a preview |
| `!` | Spam | Mail | G | — |
| `s` | Star / pin | Star (mail); pin a person (Messages); pin a source (Updates) | G, 22 | `p` pin (Updates, place); Focus skip moves to `j` |
| `I` / `U` | Mark read / unread | Mail | G | Reading reader's `I` (issue) becomes a tab |
| `l` / `v` | Label / move to label | Mail | G | — |
| `x` / `V` / `*` | Select one / range / by pattern | Every list, the modes included | G, V | — |
| `u` (and `z`) | Undo | Everywhere; `z` is Gmail's alias in both clients | V, G | TUI `z z` retires |
| `t` | Make it a to-do | Mail, Now, Messages, Updates ("needs me"), Reading, Archive | 22 (Gmail `Shift+T`) | — |
| `X` | Take it out of this mode | Picker: lowercase letter moves the email, capital moves the sender, `Enter` means "not here" with no destination | 22 | `T` pass to mode merges in; Not-sure `m x u r e` and new-sender `1`–`4` answer through `X` |
| `p` | The person or issuer behind this | Sender profile, person page, issuer page | 22 | — |
| `P` | This sender, always | Mode, never/always, lead time, mute, never summarise | rule 4 | `K` sender menu and tune; `W` whois joins the person page |
| `b` | Not now | One picker: "Later (no time)" first, then times. Reply later (Messages), read later (Reading), snooze (mail, Now), schedule (To do) | G | `Z` snooze and schedule merge in (D173) |
| `,` | Correct what mxr extracted | To-do fields, record fields, mark a record checked | 22 | Archive `v` checked and To do `v` details merge in |
| `.` | Got it | Messages | 22 | — |
| `>` | Quote the selection into the reply | Messages, reader | 22 | — |
| `"` | Highlight the passage | Reading reader | new | Reading `h` |
| `y` / `Y` | Copy the most useful thing / everything | Code (Updates), reference (Archive), link (links list); `Y` copies the whole card | V yank | List `y` summarise moves to `B` |
| `B` | Brief me | Thread briefing and summary, one verb | A | Reading `B` Later shelf becomes a tab |
| `D` | Unsubscribe | With evidence and a preview | A | Assistant `D` delete moves to `#` |
| `A` `L` `R` `H` `M` `S` `Q` `F` | Another part of this email | Attachments, links (and the article or tracker page), reading layout, HTML, remote images, signatures, quoted text, full width | A, rule 4 | Vim viewport `H` `M` `L` retire; To do's `M` and `S` (Microsoft To Do) move to the palette |
| `m` | Mark read and archive | Mail | mxr | — |
| `w` | Wake: bring it back now | Snoozed: back in the inbox now. Reply queue: out of the queue. It undoes a Not now (`b`), and `b` sets it again | mxr | The web desk's `w` (Done) moves to `e` |
| `d` | Draft in your voice | Focus | mxr | — |
| `C` `T` `W` `Z` | Free | Kept free for verbs that earn a key | — | — |
| `1`–`9` | The Nth visible option | A mode's tabs (To do: Runway, Catch-up, Expired; Reading: Edition, Later, Highlights); picker options; snooze presets | HEY | Global screen digits `1`–`7` move to `g` |
| `/` | Search here | Search; Archive's ask box | G | — |
| `?` / `:` | Help / palette | Everywhere; the palette is also `Mod+K` in every client (`Ctrl-k` in the TUI) | G, aerc | TUI gains `:` and `Ctrl-k`; web's mac-only `Ctrl+P` and the TUI's `Ctrl-p` stay as aliases |
| `@` | The assistant | Open or focus the assistant | new | TUI `Ctrl-a` (screen's prefix, and add-Cc in send confirm) and web `Mod+.` |

Four results come out of this table.

- **Every mail and mode context uses the same letters.** Now, Messages, To do, Updates, Reading and Archive differ only in what `Enter`, `e`, `,` and the tabs mean there.
- **Every capital is in one of rule 4's four groups.**
  - `E`, `O`, `P`, `Y` and `V` are wider forms of their lowercase verbs.
  - `A`, `L`, `R`, `H`, `M`, `S`, `Q` and `F` show another part of the email.
  - `G`, `J`, `K`, `N`, `I`, `U` and `D` are frozen keys from Gmail, vim or A005.
  - `X` and `B` are the two named mxr verbs.
- **The digits stop meaning seven different things.** A digit always picks the Nth thing you can see.
- **Four letters are free: `C`, `T`, `W` and `Z`.** They can go to a future verb without moving anything. `w` went to Wake on 28 October 2026, once Snoozed and the reply queue had it in both clients, and `d` to Draft in your voice in Focus & reply.

### `g` goes to places, and the case says what kind

`g` plus a lowercase letter goes to mail places and modes. `g` plus a capital goes to tools. Gmail's letters stay where they are.

| Lowercase: mail and modes | Capital: tools |
|---|---|
| `g h` Now, `g m` Messages, `g x` To do, `g u` Updates, `g r` Reading, `g e` Archive | `g A` Analytics, `g L` Logs, `g Y` Activity (was `g y`) |
| `g i` Inbox, `g s` Starred, `g t` Sent, `g d` Drafts, `g a` All Mail, `g l` labels | `g R` Rules, `g D` Diagnostics, `g P` Deliveries (were digits `3`, `5`, `7`) |
| `g n` Snoozed, `g #` Trash, `g !` Spam | `g C` Settings and accounts (was `g c`, `0` and digit `4`) |
| `g q` Reply later, `g o` Owed, `g w` Waiting on, `g v` Invites | `g S` Screener (`8` retires) |
| `g f` Filter this list, `g F` Focus and reply, `g 1`–`g 9` saved searches, `g g` top | |

This retires the overdue aliases `g R` and `g P`, which were meant to go after 0.6.41, and `g E`, `g p`, `g H`, `0`, `8` and `9`. Raw headers (`g H`) move to the palette.

### The modes, side by side

The verbs are shared, so a mode's map is short: what `Enter`, `e`, `,` and the tabs mean, and which verbs apply.

| Mode | `Enter` | `e` | Tabs (`1`, `2`, `3`) | Mode-only verbs |
|---|---|---|---|---|
| Now | Open in its mode | Done here | — | — |
| Messages | Open the person | Done | — | `.` got it, `>` quote, `Mod+Enter` send and next, `{` `}` topic |
| To do | Do it (pay, open the link) | Tick off | Runway, Catch-up, Expired | `,` fix fields; `Space` peeks at details |
| Updates | Show the routine | Let go of the source | — | `y` copy code, `L` tracker page, `Space` peek |
| Reading | Read | Let go | Edition, Later, Highlights | `D` unsubscribe, `L` article, `R` layout, `O` Marginalia, `"` highlight |
| Archive | Open the document | — | — | `/` ask, `y` `Y` copy, `,` fix and check, `{` `}` year |

Every mode also gets `x`, `V`, `*`, `g g`, `G`, `Ctrl-d` and `Ctrl-u`, because modes now inherit the list context. Today they have only `j` and `k`, and held keys repeat only in the mailbox list.

The TUI's place view (Paper trail bundles) follows the same verbs. Its `>` and `+` (more senders) move to the palette. Whether Paper trail stays a place at all is `docs/issues/paper-trail-hard-to-reach.md`. The invites screen's bare `a`, `t`, `m` and `d` give way to the `i` prefix, which already works everywhere. Bare `a` there collides with reply all.

## Contexts form one tree in both clients

```
global
├─ mail                         shared mail verbs, wherever an email is lit
│  ├─ list                      motions, selection, held-key repeat
│  │  ├─ mailbox                inbox, labels, search results
│  │  ├─ now  messages  todo  updates  reading  archive
│  │  └─ screener  invites
│  └─ reader
│     └─ reading-reader
├─ sidebar
├─ tool                         rules  accounts  diagnostics  analytics  deliveries  activity  logs  settings
├─ compose                      modifier chords only; letters type
├─ assistant
└─ overlay                      picker  confirm  preview  prompt  sheet
```

These are the names in both clients, in the definition and in `mxr keys`.

Resolution order:

1. A text field, or a composition in progress, owns every key except global modifier chords.
2. An open overlay owns every key except `?`.
3. Then the innermost context outwards to `global`.

A child inherits its parent's bindings and may override one only with the same verb's meaning in that place. Changing the verb fails the definition check. Tools may bind their own letters for actions that have no mail verb, such as Rules dry run, and must print them in the tool's footer. Shared verbs (`c`, `#`, `Enter`, `/`, `Esc`) keep their meaning in tools too.

### Back and quit

- `Esc` cancels or goes back one level. It never destroys anything: the send confirm's `Esc` returns to the draft.
- `q` closes the innermost layer, the way mutt does. It closes an overlay, then a sub-view such as the person view, then the reader. Only at the root does it quit the TUI. The web reader's `q` already closes.
- `h` moves left: reader to list, list to sidebar.
- `Ctrl-c` stops what is running (an assistant turn, a sync), and otherwise acts like `q` at the root.
- `Ctrl-z` suspends the TUI, as every terminal program does.

## Overlays share one grammar

| Part | Rule |
|---|---|
| Close | `Esc`, and `q` where nothing is typed, closes the overlay without changing anything. Focus returns to the element that opened it. If that element has gone, focus goes to the active pane, which every mode and tool now marks. |
| Confirm | `Enter` runs the highlighted option. In a yes or no question, `y` is `Enter` and `n` is `Esc`. A preview (dry run) shows what will change, and `Enter` commits it. There is no second confirmation. |
| Pick | Each option prints its letter on itself (HEY prints the letter on each button). Destination letters are always the `g` letters: `m` Messages, `x` To do, `u` Updates, `r` Reading, `e` Archive. Capitals apply the same choice to the sender. Digits pick the Nth option only in ordered lists. |
| Move | `j`, `k` and the arrows move. In a picker with a filter field, letters type, and `Ctrl-n` and `Ctrl-p` move. |
| Destroy | Only an option named with its object ("Discard draft", "Delete rule") destroys, and never on `Esc` or a bare `Enter` default. |
| Toggle | The key that opened a toggle sheet (`?`, `A`) also closes it. |

A closing overlay stops owning keys as soon as it starts to close. This fixes `docs/issues/menu-exit-swallows-keys.md`, where a menu in its exit animation swallowed the next `u`.

## Prefixes show what can follow

Both clients wait four seconds after a prefix, so the list of next keys can be read. This was 1,000 ms, after vim's `timeoutlen`, Zed and which-key; the web section of 26 October raised it, and the TUI followed in K4. After 300 ms a box above the status line lists the keys that can follow, from the definition with the person's keys (Helix's auto-info, Emacs 30's which-key). Typing faster than that never shows it, and `Esc` cancels the prefix. A prefix with nothing to list in the current place waits only 1,000 ms, since there is no guide to read; today that is the TUI's `z`, held so the retired `z z` can say where it went.

A key that completes no binding cancels the prefix and says so ("`g z` is not a shortcut"). Today both clients run the second key on its own, so a mistyped `g U` marks mail unread.

## One definition, read by every client

### The definition is a data file in its own crate

`crates/keymap` holds `keymap.toml` and a typed loader. It depends on no other mxr crate. Each entry gives:
- the action id;
- the verb it belongs to;
- its context;
- its default keys, per client where they must differ;
- its label (the verb as a word, for tooltips and hints);
- whether it is destructive;
- an optional `client = "web" | "tui"` with a reason.

Retired keys are listed with the action they moved to.

The crate's tests are the lint. They fail on:
- a key bound to two verbs within mail and mode contexts;
- a reserved key bound below global;
- a prefix that is also a key on its own;
- a capital outside rule 4's four groups;
- a TUI default a terminal cannot deliver;
- a web default on the reserved list, checked per platform;
- a destructive action without a preview or undo flag;
- an action with no label.

### Both clients dispatch by action id, never by key

**TUI.** The TUI resolves a key event against the context stack to an action id, then calls the handler that the context registers for that id. The mode `match` arms become handlers keyed by action. Raw key handling stays only in text widgets (prompts, the search field, forms).

`keys.toml` overrides go through the same loader. A remap therefore changes dispatch, help, the hint bar and the palette at once, in every context, modes included. Remapping `archive` would no longer silently kill `e` in Now.

**Web.** The web registry keeps its `run` and `command` handlers but takes keys, contexts and labels from generated data (`apps/web/src/lib/actions/keymap.generated.ts`, written by `mxr keys --format json`). A test fails if:
- a registry action has no definition entry;
- a definition entry has no web handler and no `client = "tui"`;
- a `keydown` listener exists outside the dispatcher and the allowed text widgets.

The allowed-listener list is the only escape hatch, so compose chords, dialog letters and assistant keys move into the definition.

**Parity.** `keymapParity.ts` and its 60 hand-kept differences are replaced by one test that runs against the definition: every action id has a handler in every client, or a declared reason why not.

### Every client uses the same keys

`Mod` is Cmd on macOS and Ctrl elsewhere. Every client binds the same key to the same action, with the same Shift and the same prefix. A client may differ only for one of two reasons, and the definition records which:

- **The client cannot deliver the key.**
  - Terminals have no Cmd, so the TUI's `Mod` is `Ctrl`.
  - Terminals can't send `Ctrl-Enter` without the kitty protocol, so the TUI sends with its own confirm.
  - A browser keeps `Tab` for focus order. `Tab` switching panes stays a TUI extra, and `h` and `l` do the same in both clients.
  - At the root of the web app, `q` has nothing to close.
  - `Ctrl-z` suspends only a terminal program.
- **The client lacks the feature.**
  - The TUI composes in `$EDITOR`, so the web composer's chords have no TUI counterpart.
  - Raycast has no reader view toggles.

Anything else that differs today is moved to match in K1 or K3. That covers:
- the assistant's newline: `Shift+Enter` in both, with `Alt-Enter`/`Ctrl-j` kept as TUI aliases;
- conversation delete, which becomes `#` in both;
- snooze presets, which are digits in both;
- web `Mod+.`, which becomes `@`.

**Clients that need a modifier use `Mod` plus the same letter.** The Raycast extension and the desktop app's menu can't take bare letters, because Raycast sends typing to its search field. They use `Mod` plus the verb's letter, and keep its Shift:

| Action | Keys |
|---|---|
| Reply | `⌘R` |
| Done here | `⌘E` |
| Done for all in view | `⌘⇧E` |
| Open in mxr | `⌘O` |
| Open outside mxr | `⌘⇧O` |
| Mark unread | `⌘⇧U` |
| Undo | `⌘Z` |
| Later | `⌘B` |

The extension already does most of this (`apps/raycast/src/now.tsx`, `messages.tsx`, `updates.tsx`, `components/ThreadDetail.tsx`). Two keys move: Reading's "Save for Later" goes from `⌘L` to `⌘B`, and "Draft in my voice" (`⌘D`) goes to the palette-only list until it earns a letter.

Raycast's own common shortcuts win where Raycast already has one for the same action, because people carry those across every extension. Copy is `⌘⇧C` and Pin is `⌘.` (Raycast's `Keyboard.Shortcut.Common`). Reply all is `⌘⇧R`, because `⌘A` selects the search text. The definition records each of these as a Raycast exception, and every other Raycast key comes from the definition. The desktop menu follows the same rule, minus the keys macOS reserves (⌘W, ⌘N, ⌘Q, ⌘,).

### The daemon serves the effective key map

Customisation is a capability, so it follows the product shape:

- `GetKeymap` IPC returns the defaults with the user's `keys.toml` applied, plus the single-key switch.
- `mxr keys` prints it.
  - `mxr keys --format json` prints the same data; it replaces `docs/reference/tui-keymap.json` and generates the site's keybindings page and the web data.
  - `mxr keys check` validates `keys.toml` with the crate's lint and names each conflict.
  - `mxr keys explain <key> [--context todo]` says what a key does where.
- The web app reads `GetKeymap` at start, so a remap in `keys.toml` applies to both clients.
- The web settings page writes the switch and remaps through the daemon's config path, with a preview of conflicts before saving.

## Teaching follows D130 and gets three additions

The places that show keys stay as D130 set them: the lit row's actions, the status strip, `?` and the palette. What changes is where their content comes from, plus three new surfaces.

**What every surface reads.**
- Help, the hint bar, the status strip, tooltips and `aria-keyshortcuts` all come from the definition for the active context. The mode is now part of that context, so Now's strip says `e done here`.
- The hand-written help tables in `crates/tui/src/ui/help_modal.rs:94-531` and the literal key chips in web components are deleted.
- The palette shows every key of an action, not only the primary. It ranks the lit item's actions first, the way Superhuman ranks by context.

**Three additions.**

1. **The prefix popup.** Described in "Prefixes show what can follow".
2. **Next time.** When an action that has a key runs from the palette or a click, the toast adds "Next time: `e`". It shows once per action per profile and is stored with the hint seen-state (chapter 22's `SetHintSeen`). NN/g recommends introducing a shortcut after the user has done the action the standard way. Superhuman's palette shows the shortcut "for next time".
3. **Moved keys.** For 30 patch releases after a key moves, pressing the old key does nothing except show where its action went: "Let go of all is now `E`". It never runs the old action, so an old habit cannot do something unexpected. Each move is an `[[action.moved]]` entry with an `until` version; the definition check fails once mxr reaches it, so the entry is deleted.

## Keys work for everyone, on every keyboard

### Accessibility

- **Single-key shortcuts can be turned off.** The web settings get an off switch, as GitHub's "Character keys" setting works. Modifier chords and the palette keep working, and `?` says the switch is off. Together with remapping through `keys.toml`, this meets WCAG 2.1.4.
- **Keys never fire while typing.** One helper (`isTypingTarget(event)`) checks `isComposing`, `keyCode === 229`, editable targets and closing dialogs. The dispatcher and every Enter-to-submit field use it.
- **Every key has a path without the keyboard**, as Primer requires: a visible control or a palette entry.

### Keyboard layouts

- Printable keys match on `event.key`.
- On macOS, Option-typed characters are matched as the character, with no `Alt` added. Elsewhere, an AltGr character is matched the same way.
- New keys prefer letters. Each symbol key (`#`, `!`, `[`, `]`, `{`, `}`, `"`, `>`, `@`, `,`, `.`, `;`) also has a palette entry.

A per-user layout setting, as Figma chose after automatic detection failed, is deferred until someone reports a layout that remapping cannot fix.

### Terminal and browser keys

**TUI defaults never use:**
- `Ctrl-i`, `Ctrl-m`, `Ctrl-h` and `Ctrl-[`, which arrive as `Tab`, `Enter`, `Backspace` and `Esc`;
- `Ctrl-s` and `Ctrl-q` (flow control), and `Ctrl-b` and `Ctrl-a` (tmux and screen prefixes);
- any `Ctrl-Shift` combination;
- `Alt`, which macOS terminals send only with "Option as Meta".

**TUI behaviour:**
- When crossterm's `supports_keyboard_enhancement()` says the terminal can, the TUI pushes `DISAMBIGUATE_ESCAPE_CODES`, so an `Esc` keypress never waits for a possible Alt sequence. No default depends on the enhancement.
- Bracketed paste is on.
- `mxr keys check` warns about remaps a terminal cannot deliver.

**Web:**
- The reserved list exists once (`browserReserved.ts`), and compose chords are checked against it now that they are in the definition. Today `Mod+s` and `Mod+Shift+R` in compose escape the guard.
- **Tauri.** The desktop shell builds a native menu from the definition's app-level commands, with accelerators. Following the HIG, it lists modifiers in Control, Option, Shift, Command order and does not reuse ⌘W, ⌘N, ⌘Q or ⌘,. macOS App Shortcuts can then remap those commands.
  - As built in K4 (`apps/desktop/src-tauri/src/menu.rs`): View, Go and Message menus sit between the standard Edit and Window menus. Go is every `g` place from the definition, tools after a separator; Message and View list action ids that a test holds to the definition. A command's title is its action's label, so an App Shortcuts binding survives a key remap. Only actions whose web key is already a modifier chord get an accelerator (`⌘K` palette, `⌘.` assistant). Command plus the verb's letter, as the Raycast table above gives, is not used: a menu accelerator takes the key whenever the page leaves it unhandled, and `⌘B`, `⌘Z` and `⌘A` belong to the composer and text fields. A command runs through the web registry by id, as its key would.

Chromium reserves no keys in an installed app window. Whether Tauri's WKWebView frees `Mod+1`–`9`, `Mod+T` and `Mod+L` is a spike, S-K1. Until it is answered, the web app treats the desktop window like a browser tab.

## Adding a key is a checklist

A PR that adds or changes a key answers these in its description, and the definition lint enforces what it can.

1. **Is it an existing verb in a new place?** Use the verb's key, and write what the verb means there.
2. **Is it a new verb?** Put it in the palette. It takes a single key only with evidence that it is part of a daily flow. The evidence can be from dogfooding, from the activity log on BK's machine, or from the mode's design. The key must be one of the free letters, or the Shift of a related verb under rule 4.
3. **Does it destroy anything?** Then it needs a preview or undo. The action is flagged destructive, and an overlay option names its object.
4. **Does it differ between clients?** Then declare it in the definition with the reason.
5. **Does it work everywhere?** It must pass the lint for terminal and browser keys, and every symbol key needs a palette path.
6. **Did the docs follow?** Help, the palette, the hint bar and the site page are generated. The mode's guide copy (`crates/protocol/src/types/mode_guide.rs`) names keys only through definition lookups, never as literals, so it cannot drift.

## Delivery is four rungs, and the first changes no key

### K1: One definition, same keys

K1 lands in four pieces:
- **K1a:** `crates/keymap/keymap.toml` holds today's keys for both clients. Tests hold it equal to the TUI's dispatcher probe and to the web registry. `GetKeymap` and `mxr keys` serve it.
- **K1b:** the web registry reads its keys from the definition, and the parity test moves onto it.
- **K1c:** the TUI dispatches by action id, with mode contexts, and gains mode-aware help and hints, `keys.toml` through the shared loader, and `mxr keys check`.
- **K1d:** the web listeners outside the registry move in, the dead `place` scope goes, and Raycast reads the definition.


- Create `crates/keymap` holding today's keys exactly as they are.
- Port the TUI to dispatch by action id. The TUI modes become contexts.
- Generate the web data. The Raycast extension reads its keys from the same generated data.
- Move the roughly 40 web listeners into the definition.
- Delete the dead `place` scope.
- Add `GetKeymap` and `mxr keys` (`list`, `check`, `explain`, `--format json`).
- Make help, the hint bar and the palette mode-aware.
- Replace the parity test.

Acceptance:
- `mxr keys --format json` lists every binding today's `tui-keymap.json` lists, with the same actions, plus every web binding.
- The TUI and web Vitest key tests pass unchanged.
- Now's hint bar shows `e done here`.
- A `keys.toml` remap of `archive` changes the list and leaves Now's `e` working.
- Driven through `mxr keys explain e --context todo`, the answer is "tick off".

### K2: Safety and correctness, still the same keys

- `Esc` never discards.
- `#` previews everywhere.
- History delete moves behind a preview.
- The IME helper is in every field.
- The Alt layout fix lands.
- Held keys repeat in the modes.
- Focus returns in the modes and tools.
- A closing overlay releases keys.
- A dead-end prefix is cancelled, not run.
- `q` closes the innermost layer.
- Bracketed paste and keyboard enhancement are on in the TUI.
- `Ctrl-z` suspends.

Acceptance:
- One Vitest case per fixed web defect.
- One TUI runner test per fixed TUI defect.
- One Playwright journey: close a menu, press `u` at once, and the undo toast appears.

### K3: The key map moves to the target

- Apply the verb table, the `g` table and the overlay grammar.
- Turn on the moved-key hints and the "Next time" toast.
- Regenerate the docs.
- Rewrite chapter 22's key table as a pointer to this chapter.

Acceptance:
- The lint passes with no exceptions file.
- Every retired key shows its moved-key hint and runs nothing.
- `site/src/content/docs/reference/keybindings.md` is fully generated, the TUI tables included.
- BK triages for one day in each client using only keys. It passes if no action needs the mouse and no key's result surprises. Each surprise is logged in `docs/dogfooding-log.md`.

### K4: Off switch, remapping in the web app, prefix popup, desktop menu

- Single-key off switch.
- Web remap editor through the daemon.
- The prefix popup in both clients.
- The Tauri menu.
- Spike S-K1.

Acceptance:
- With single keys off, `e` does nothing and `Mod+K` → "Archive" works.
- A remap saved in web settings applies in the TUI on its next start.
- Holding `g` for 300 ms lists the places.
- In the desktop app, macOS App Shortcuts can rebind a menu command.

Docs ship with each rung: the guides that name keys, `reference/keybindings.md`, `reference/config.md` (`keys.toml`, the switch), and chapters 08, 12 and 22 pointing here.

## Decisions

D163 to D173 are recorded in [15-decision-log.md](15-decision-log.md). In summary:

| ID | Decision |
|---|---|
| D163 | A key names a verb and a verb has one key in every mail and mode context; modes specialise the verb, never replace it |
| D164 | Single keys are earned by frequency; the palette is the default home for a new action |
| D165 | Shift means the same verb further; every other capital is a frozen Gmail, vim or A005 key or a reader view |
| D166 | `g` lowercase goes to mail places and modes, `g` capital to tools; digits pick the Nth visible option |
| D167 | Contexts are one tree in both clients; modes inherit the list; `?` `/` `:` `g` `Esc` `u` are reserved |
| D168 | One overlay grammar; `Esc` never destroys; destination letters are the `g` letters everywhere |
| D169 | One key definition in `crates/keymap`, dispatched by action id in every client and served by the daemon through `GetKeymap` and `mxr keys`; every client uses the same key for the same action, and modifier-only clients (Raycast, the desktop menu) use `Mod` plus the same letter |
| D170 | Single-key shortcuts can be turned off and remapped in both clients (WCAG 2.1.4), with layout, IME and terminal rules enforced by lint |
| D171 | Retired keys show where their action went for two releases and never run it |
| D172 | Keys move in four rungs, and the first two change no key |
| D173 | Not now is one verb on Gmail's `b`: a picker with Later (no time) first, then times; `Z` is freed |

## Open questions

1. **Spike S-K1:** does the Tauri window receive `Mod+1`–`9`, `Mod+T` and `Mod+L`, so the desktop app can use them where a browser tab cannot?
   - Answered 2026-10-28: yes, on macOS. The K4 desktop build (debug, Tauri 2.12.4, WKWebView) loaded a local page that logged every `keydown`, and System Events sent `⌘1` to `⌘9`, `⌘T` and `⌘L` to its window. Each arrived with `metaKey` true, and neither the shell nor WebKit acted on any of them. The same run showed that the menu's accelerator and the page do not both act: `⌘K`, which the page handled with `preventDefault`, never reached its menu command, and `⌘.`, which the page left alone, ran the menu's assistant command after the page saw it. System Events sends the digits with keypad key codes (`Numpad1`), and the dispatcher matches `event.key`; a check with a physical keyboard and on Linux and Windows is still to do. No key moved because of it.
2. **The assistant key.** `@` is proposed because it is free and suggests addressing the assistant. On German and French layouts it needs AltGr, which K2's layout fix makes reachable. Confirm during K3 dogfooding, or pick another free key.
