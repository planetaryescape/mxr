# 33. Account identity

People with several accounts read Now, the five modes and search as one stream. Each email has to say which account it came to without being opened, and the answer has to look the same in the web app, the TUI and the CLI. This chapter is the one design for that (D147). The mockups are in the design-gate comp, `mxr account identity` (2026-10-22).

## The daemon owns each account's identity

An account has an identity: a **colour** and a **tag**.

- **Colour** is one of eight named slots: `sky`, `amber`, `green`, `yellow`, `blue`, `vermilion`, `purple`, `slate`. The wire carries the slot name, never a hex value. Each client maps the name to its own theme: web tokens, TUI theme colours. So a colour reads correctly in Midnight, Light, Paper and Eclipse, and in a 16-colour terminal.
- **Tag** is one to three characters, uppercase letters and digits, shown on every marked row. It carries identity by itself, so colour is never the only cue (WCAG 1.4.1).

The daemon resolves both and sends them on `AccountSummaryData.identity` (`AccountIdentityData { color, tag }`). Clients join rows to identities by `account_id`, which nearly every row payload already carries. The colour is not copied onto every row. A client that reads an older daemon without `identity` draws no account mark.

### Colours are assigned once and never move

When an account is created, the daemon stores the first free slot, in the order above, in `accounts.identity_color`. Existing accounts get slots in creation order when the column is added. Adding, removing or renaming another account never changes an account's colour, so the colour can become memory. From the ninth account on, slots repeat in order and the tag tells those accounts apart. Settings say so beside the picker.

### Tags come from the name, once

An account's default tag is derived from its name when the account is created, and the daemon stores it with the account. The initials of the name's first two words come first, so "Work" is `W` and "Planetary Escape" is `PE`. If that tag is taken, the name's first two letters are tried, then its first three, then the initials with a digit. The derivation avoids every stored tag and every configured tag, so a new account never takes one of those. Once stored, the tag is never derived again. Renaming an account, removing another, or overriding a tag in config cannot change it. A config tag that another account already holds is refused: the daemon logs an error, keeps the stored tag, and the upsert and the `identity` command refuse the same clash.

### People can change both

`[accounts.<key>]` takes `color = "<slot>"` and `tag = "<1-3 chars>"`. A config value wins over the stored slot and the derived tag. Config keeps the value as written. The daemon ignores an unknown slot, a bad tag or a clashing tag, warns with the key and the allowed values, and uses the stored value. Two accounts may be given the same colour on purpose. Settings warn but allow it. The write paths (`mxr accounts identity`, the upsert) refuse a bad new value, or a tag another account holds, before saving. The web settings picker, `mxr accounts identity <key> --color <slot> --tag <tag>` (with `--dry-run`) and the TUI account form all write these keys through the daemon's account-config path.

## A row is marked only when the view mixes accounts

Rows carry the mark when the user has more than one enabled account and the view is not scoped to one of them: the web's "All accounts", the TUI's all-accounts views, CLI output without `--account`. Otherwise no row is marked, and one-account users never see the mark. The rule depends on the accounts and the scope, not on which rows happen to be loaded, so a list never gains or loses marks while it pages.

Some places always show the account, whatever the rule:

- The reader header: the chip and "In Work · bk@company.example".
- Compose and reply: the From control, chip plus name and address, because sending from the wrong account is the costly mistake. A reply starts from the account the mail came to.
- The account switcher and settings: every account with its chip, and a strip of the colours for "All accounts".

## Where the mark sits on each surface

**Web.** `AccountMark` renders the chip, a rounded block in `--account-<slot>` with the tag in `--account-<slot>-ink`. It is part of `Row` (chapter 27), not a mode's slot: it sits in the trail before the time on every row of every list (Now, Messages, To do, Updates, Reading, Archive, search and mailboxes, the desk, drafts, deliveries, invites, the screener) and on the assistant's readout rows. The row's leading edge stays the unread bar's and the lit hairline's. Its accessible name is the account's name, so a screen reader hears "Work" where a sighted reader sees `W`. The status bar's lit-row line starts with the chip and the account name. Themes define the eight `--account-*` pairs in `themes.ts`, generated like every other token. Light themes use deeper fills with light text and dark themes lighter fills with dark text, each pair at 4.5:1 or better.

**TUI.** The same chip is a four-cell block in the account colour holding the tag, in the row's trailing column before the date, and in the reader header and compose From. The theme maps each slot to truecolor and to a 256-colour nearest pick tested to keep the eight apart. With 16 colours or `NO_COLOR`, the column shows `[W]` in bold: the tag alone separates accounts, so nothing is lost.

**CLI.** `mxr accounts` lists each account's tag and colour. Tables of mail (search, the mode commands, Now) add an `ACCT` column with the tag under the same mixing rule, as plain text with no colour codes. JSON and JSONL rows keep `account_id`. `mxr accounts --format json` carries `identity`, so scripts join the two the same way the apps do.

## The account palette is reserved for accounts

Labels keep their own colours and sender avatars keep the chart colours. The account palette is used for nothing else: no status, no label, no chart. On a row, the account chip is the only filled block holding letters in the trail. That shape keeps it apart from a label badge even when the hues are close.

## What this changes elsewhere

- **27 Web design system:** the row's trail gains the account mark, and the token list gains the account pairs.
- **12 Config:** `[accounts.<key>] color` and `tag`.
- **08 TUI and 09 CLI:** the mark's column and its fallbacks.
- **Protocol:** `AccountSummaryData.identity`. `AssistantReadoutRowData` gains `account_id` so readout rows can be marked.
- **Store:** `accounts.identity_color`.
