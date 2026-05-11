# Web / TUI parity matrix

Code-verified on 2026-05-11 against:

- TUI action surface: `crates/tui/src/action.rs`
- TUI command palette: `crates/tui/src/ui/command_palette.rs`
- Web command palette: `apps/web/src/features/command-palette/CommandPalette.tsx`
- Web global keymap: `apps/web/src/lib/keymap.ts`
- HTTP bridge routes: `crates/web/src/lib.rs`, `crates/web/src/routes_v6.rs`

## Status legend

| Status | Meaning |
|---|---|
| Covered | Web has route/UI, command or key where expected, and bridge support. |
| Partial | Web has part of the workflow, but misses command coverage, keyboard flow, scope, or depth. |
| API-only | Bridge route exists, but web has little or no user-facing workflow. |
| Missing | No meaningful web route/UI found. |

## Highest priority gaps

| Gap | Current web state | Why it matters | Next slice |
|---|---|---|---|
| Command/action bus | Web palette is mostly navigation plus compose/new rule/new account. Context actions live inside individual pages. | TUI exposes actions consistently through keybindings and palette. Web cannot reach many already-implemented workflows from `Cmd-K`. | Add a shared web action registry consumed by command palette, help, and keymap. Seed it with existing routes and page-level actions. |
| Context command coverage | Thread actions such as summary, sender profile, attachments, export, unsubscribe, labels, links, browser-open are not palette-addressable. | The TUI command palette makes these discoverable without learning every page toolbar. | Add route-aware thread/mailbox commands after the registry exists. |
| Search/saved-search depth | Search has mode/sort, token chips, saved-search create, and preview pane. Missing scope picker, keyboard result navigation, and saved-search edit/delete/pin/color UX. | Search is navigation in mxr; saved searches are core inbox lenses. | Add `scope`, j/k result focus with preview sync, and saved-search management. |
| Semantic controls | Web diagnostics show semantic status; bridge has enable/reindex/profile routes. No Settings/Diagnostics controls. | Semantic lifecycle is an mxr-platform feature, not just diagnostics. | Add Settings or Diagnostics semantic panel with enable, reindex, and profile actions. |
| Account config editing/repair | Web detail supports test/default/reauth/disable/remove/aliases. Missing full config edit, repair, refresh workflow. | TUI account page can fix unhealthy config without dropping to CLI. | Extend account detail around `/platform/accounts/config` and repair request. |

## Global commands and navigation

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Compose | `/compose/new`, launcher modal | `Compose` | `c` | `/mail/compose/session*` | Covered | Compose page is full-page and editor-backed. |
| Go to Inbox | `/m/inbox` | `Go to inbox`, shell lenses | `g i`, `1` | `/mail/mailbox` | Covered | Sidebar lenses also route to mailboxes. |
| Go to Starred | `/m/starred` | `Go to starred` | `g s` | `/mail/mailbox` | Covered |  |
| Go to Sent | `/m/sent` | `Go to sent` | none | `/mail/mailbox` | Partial | Palette command exists, no global shortcut. |
| Go to Drafts | `/m/drafts` | `Go to drafts` | `g d` | `/mail/drafts`, `/mail/mailbox` | Covered |  |
| Go to All Mail | `/m/archive` | shell lens if exposed | `g a` | `/mail/mailbox` | Partial | Web palette currently labels `g a` for Analytics, but global keymap uses `g a` for archive/all-mail. |
| Open subscriptions | `/subscriptions`, analytics subscriptions dashboard | shell lens or route | `g u` | `/platform/subscriptions`, `/mail/actions/unsubscribe` | Partial | Route exists; palette action is missing. |
| Open reply queue | `/reply-queue` | none | `g l` | `/mail/reply-later` | Partial | Shortcut and route exist; palette action missing. |
| Open screener queue | `/screener` | `Screener` | `5` | `/mail/screener/queue`, `/mail/screener/decisions` | Partial | First-account only. |
| Open diagnostics | `/diagnostics` | `Diagnostics` | none | `/admin/status`, `/admin/diagnostics`, `/admin/logs`, `/admin/events` | Partial | No section navigation/detail panes. |
| Open settings | `/settings/*` | `Settings` plus setting subcommands | none | local prefs plus platform LLM config | Partial | Keybindings page is a short static list. |
| Help | Help dialog | no command row | `?` | none | Partial | Help is searchable/contextual, but not generated from full action registry. |
| Sync now | mailbox sync button/progress surfaces | none | none | `/mail/sync` | Partial | API exists and status surfaces; no palette command. |
| Quit view/back | browser navigation | none | `Esc` on some dialogs, `u` in reader | client-side | Partial | Not normalized across pages. |

## Mailbox and reader actions

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Archive | mailbox/thread/bulk buttons | none | reader `e` | `/mail/mutations/archive` | Partial | UI exists; command palette action missing. |
| Trash | mailbox/thread/bulk buttons | none | reader `Delete`/`Backspace` | `/mail/mutations/trash` | Partial | Bulk confirm exists. |
| Spam | mailbox/thread/bulk buttons | none | reader `!` | `/mail/mutations/spam` | Partial | Bulk confirm exists. |
| Star | mailbox/thread/bulk buttons | none | reader `s` | `/mail/mutations/star` | Partial |  |
| Mark read/unread | mailbox/thread/bulk buttons | none | reader `m` | `/mail/mutations/read` | Partial |  |
| Mark read and archive | no direct web action | none | none | `/mail/mutations/read-and-archive` | API-only | Bridge supports it; web has no direct control. |
| Undo mutation | toast action, mutation helper | none | none | `/mail/mutations/undo` | Partial | Exists when mutation returns `mutation_id`; no durable undo surface. |
| Apply label | no web label picker | none | none | `/mail/mutations/labels` | API-only | Route exists; no picker/workflow. |
| Move to label/folder | no web move picker | none | none | `/mail/mutations/move` | API-only | Route exists; no picker/workflow. |
| Snooze | thread and bulk dialog | none | reader `z` | `/mail/actions/snooze`, `/mail/actions/snooze/presets` | Covered | Custom natural-language time and presets exist. |
| Unsnooze | snoozed mailbox route if listed | none | none | `/mail/snoozed/{message_id}/wake` | API-only | No clear web workflow observed beyond route support. |
| Reply | thread toolbar | none | reader `r` | `/mail/compose/session*` | Covered | Opens compose with reply context. |
| Reply all | thread toolbar | none | reader `a` | `/mail/compose/session*` | Covered |  |
| Forward | thread toolbar | none | reader `f` | `/mail/compose/session*` | Covered |  |
| Draft assist | compose/thread API route | none | none | `/mail/threads/draft-assist` | API-only | No visible web control found. |
| Summarize thread | thread toolbar opens right rail | none | none | `/mail/threads/{thread_id}/summarize` | Partial | UI exists; no command/shortcut. |
| Sender view | thread toolbar opens right rail | none | none | `/mail/sender` | Partial | No standalone sender route/palette command. |
| Unsubscribe | analytics subscriptions confirm flow | none | none | `/mail/actions/unsubscribe` | Partial | Not available from reader/message context. |
| Attachments | thread message attachment buttons/right rail | none | none | `/mail/attachments/open`, `/mail/attachments/download` | Partial | No TUI-like attachment list command. |
| Open links | no link list command | none | none | none found in bridge route inventory | Missing | Needs HTML/link extraction workflow or client-side link list. |
| Open in browser | no reader/browser-open action | none | none | none found in bridge route inventory | Missing | TUI has command; web may not need provider browser-open, but parity is absent. |
| Toggle reader/html/remote images | reader toolbar | none | remote switch, mode buttons | client-side | Partial | UI exists; TUI shortcuts are not mirrored. |
| Toggle signature | no web control found | none | none | client-side | Missing |  |
| Export thread | no web control found | none | none | `/mail/threads/{thread_id}/export` | API-only | Bridge route exists. |
| Toggle fullscreen | responsive split layout | none | none | client-side | Missing | No explicit fullscreen action. |
| Visual/select/batch | mailbox selection + bulk bar | none | row selection only | mutation endpoints | Partial | Bulk bar exists; keyboard selection parity is incomplete. |

## Search and saved searches

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Open global search | search palette and `/search` | `Search` | `/`, `2` | `/mail/search` | Covered |  |
| Filter current mailbox | search page keeps optional account only | none | none | `/mail/search` | Partial | No mailbox-local filter affordance. |
| Cycle search mode | search mode select | none | none | `/mail/search?mode=` | Partial | UI exists; no shortcut. |
| Next/previous result | hover/focus preview | none | none | client-side | Partial | Preview pane exists; no j/k result-reader sync. |
| Search preview | preview pane on wide screens | none | focus/hover only | `/mail/threads/{thread_id}` | Covered | Added as split search preview. |
| Create saved search | save dialog | none | none | `/platform/saved-searches/create` | Partial | Create only. |
| Edit saved search | no web form found | none | none | no update route found | Missing | Protocol has save form actions in TUI; bridge exposes create/delete/run/list. |
| Delete saved search | API helper exists, no visible UI | none | none | `/platform/saved-searches/delete` | API-only |  |
| Run saved search | sidebar lenses route | shell lenses | saved-search lens click | `/platform/saved-searches/run`, `/mail/mailbox` | Partial | Sidebar lens is usable; no explicit run command. |
| Pin/color/reorder saved search | no web UI found | none | none | no route found | Missing |  |
| Search scope | no UI control | none | none | `/mail/search?scope=` | API-only | `threads/messages/attachments` supported by API helper. |

## Analytics

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Open analytics dashboards | `/analytics/{dashboard}` tabs | `Analytics` | `3` | analytics routes | Covered | Six dashboards exist. |
| Storage group by | storage select | none | none | `/platform/analytics/storage-breakdown` | Covered |  |
| Storage keyword/filter | storage input | none | none | client-side | Covered |  |
| Storage drilldown | right rail on chart click | none | none | client-side | Partial | Drilldown is summary-only. |
| Largest messages | storage dashboard panel | none | none | `/platform/analytics/largest-messages` | Covered |  |
| Stale perspective | toggle group | none | none | `/platform/analytics/stale-threads` | Covered |  |
| Stale window controls | no web controls found | none | none | endpoint supports params | Partial | TUI can adjust older/within day windows. |
| Contacts mode/drilldown | asymmetry + decay panels | none | none | `/platform/analytics/contact-*` | Partial | Threshold exists; no TUI-like contact mode/drilldown. |
| Refresh contacts | no web control found | none | none | `/platform/analytics/refresh-contacts` | API-only |  |
| Response direction | toggle group | none | none | `/platform/analytics/response-time` | Covered |  |
| Subscriptions rank | open-rate sort in dashboard | none | none | `/platform/subscriptions` | Covered |  |
| Subscription unsubscribe | confirm modal | none | none | `/mail/actions/unsubscribe` | Covered | Dashboard-only. |
| Wrapped range/window | global range select | none | none | `/platform/analytics/wrapped` | Partial | No TUI-like story mode/tile navigation/year stepping. |
| Rebuild analytics | header button | none | none | `/platform/analytics/rebuild` | Covered |  |

## Rules

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Rules list | `/rules` | `Rules` | `g r`, `4` | `/platform/rules` | Covered |  |
| New rule | `/rules/new` | `New rule` | none | `/platform/rules/upsert-form` | Covered |  |
| Edit rule | `/rules/{id}` | none | none | `/platform/rules/form`, `/platform/rules/upsert-form` | Covered | Route exists; palette action missing. |
| Toggle rule | list/detail switch | none | none | `/platform/rules/upsert-form` | Covered |  |
| Delete rule | list action | none | none | `/platform/rules/delete` | Covered |  |
| Rule dry-run | detail panel | none | none | `/platform/rules/dry-run`, `/mail/search` for new rules | Partial | Shows raw JSON preview. |
| Rule history | detail panel | none | none | `/platform/rules/history` | Partial | Shows raw JSON. |
| Typed rule builder | simple text inputs/buttons | none | none | `/platform/rules/form` | Partial | Action is free text with common quick buttons. |
| Apply now | detail button for six mail actions | none | none | mail mutation endpoints | Partial | Missing label/move/non-mail actions. |

## Accounts and onboarding

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Accounts list | `/accounts` | `Accounts` | none | `/platform/accounts` | Covered |  |
| New account/onboarding | `/accounts/new`, `/onboarding` | `Add account` | none | auth/session/account routes | Covered |  |
| Test account | account detail button | none | none | `/platform/accounts/test` | Covered |  |
| Set default account | account detail button | none | none | `/platform/accounts/default` | Covered |  |
| Reauthorize account | account detail OAuth flow | none | none | `/platform/auth/sessions/*` | Covered |  |
| Disable/remove account | account detail buttons | none | none | `/platform/accounts/{key}/disable`, `DELETE /platform/accounts/{key}` | Covered | Remove uses browser confirm. |
| Account aliases | account detail alias panel | none | none | `/platform/accounts/{id}/addresses*` | Covered |  |
| Account config edit | no full edit form found | none | none | `/platform/accounts/config`, `/platform/accounts/upsert` | Partial | Config read/upsert exist; web detail reconstructs limited config for test/reauth. |
| Repair account | no web control found | none | none | protocol supports repair; bridge route not found | Missing | TUI exposes unhealthy-account repair. |
| Refresh accounts | query reload only | none | none | `/platform/accounts` | Partial | No explicit refresh command. |

## Diagnostics, semantic, settings

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Diagnostics status | `/diagnostics` panels | `Diagnostics` | none | `/admin/status`, `/admin/diagnostics` | Partial | No section/detail navigation. |
| Logs/events | diagnostics panels | none | none | `/admin/logs`, `/admin/events` | Partial | Inline panels only; no open logs command. |
| Generate bug report | diagnostics button | none | none | `/admin/diagnostics/bug-report` | Covered | Copies to clipboard. |
| Open config | no web config editor | none | none | mixed endpoints | Partial | LLM config is covered; general config is not. |
| Semantic status | diagnostics panel | none | none | `/platform/semantic/status` | Covered | Status only. |
| Enable semantic | no web control found | none | none | `/platform/semantic/enable` | API-only |  |
| Reindex semantic | no web control found | none | none | `/platform/semantic/reindex` | API-only |  |
| Install/use semantic profile | no web control found | none | none | `/platform/semantic/profiles/install`, `/platform/semantic/profiles/use` | API-only |  |
| LLM status/config | Settings > LLM | `LLM settings` | none | `/platform/llm/status`, `/platform/llm/config` | Covered | Saves config through daemon and reloads provider. |
| Keybindings settings | static settings page | settings subcommand | `?` for help | client-side | Partial | Does not reflect `keys.toml` or full TUI action map. |
| Snippets | Settings > Snippets | settings subcommand | none | `/mail/snippets` | Covered | No TUI-like modal shortcut. |

## Triage workflows

| TUI action or command | Web route/UI | Web command | Web shortcut | API endpoint | Status | Notes |
|---|---|---|---|---|---|---|
| Flag reply later | no direct reader action found | none | none | `/mail/reply-later/{message_id}` | API-only | Reply queue can clear, but reader cannot flag. |
| Reply queue | `/reply-queue` list/remove/open | none | `g l` | `/mail/reply-later` | Partial | Missing split detail pane and modal keyboard workflow. |
| Screener queue | `/screener` list/decide | `Screener` | `5` | `/mail/screener/queue`, `/mail/screener/decisions` | Partial | First account only; no list decisions/clear workflow. |
| Screener allow/deny/feed/paper-trail | row buttons | none | none | `/mail/screener/decisions` | Partial | No keyboard commands. |
| Sender view | thread right rail | none | none | `/mail/sender` | Partial | No standalone sender route/search entry. |

## Recommended next implementation slice

Start with command/action parity because most missing rows already have routes or page UI.

Acceptance criteria:

- Add a shared web action registry that exposes id, label, category, shortcut, route/context requirements, and run handler.
- Make command palette, help dialog, and global keymap consume the registry instead of hardcoding overlapping lists.
- Fix the current `g a` mismatch: global keymap sends `g a` to archive/all-mail, while the palette labels Analytics as `g a`.
- Add palette commands for existing route-level workflows: Reply Queue, Subscriptions, Snoozed, Diagnostics bug report, Settings > LLM, Settings > Snippets, Analytics dashboards.
- Add route-aware commands for existing thread workflows: Summary, Sender, Attachments, Archive, Trash, Spam, Star, Read/Unread, Snooze, Reply, Reply all, Forward.
- Add commands for API-only but simple semantic lifecycle controls: Enable, Disable, Reindex, Use profile.
- Cover with behavior tests through `CommandPaletteMount` and public stores/router surfaces, not helper internals.

After that slice, close Search/Saved Searches, then Semantic controls, then Account config/repair.
