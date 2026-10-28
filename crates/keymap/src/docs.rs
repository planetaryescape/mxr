//! The TUI half of `site/src/content/docs/reference/keybindings.md`, written
//! from the definition so the published tables can't drift from it,
//! overlays included. The web
//! half comes from the web registry (`apps/web/src/lib/actions/keybindingsDoc.ts`).

use crate::{place_title, KeyAction, Keymap, TUI_CONTEXTS};
use std::collections::BTreeSet;

/// The mail contexts a key must work in to be listed once under
/// "Everywhere in mail". The screener is a queue overlay with its own few
/// keys, and Focus & reply has no list to select in.
fn mail_contexts() -> impl Iterator<Item = &'static str> {
    TUI_CONTEXTS
        .into_iter()
        .filter(|c| *c != "screener" && *c != "focus")
}

/// The places that show an email, whose shared verbs are listed once.
const ON_AN_EMAIL: [&str; 3] = ["list", "reader", "place"];

/// Each place's table, in page order, with what to say above it.
pub(crate) const PLACES: &[(&str, &str)] = &[
    ("list", "Inbox, labels and saved searches."),
    ("reader", "An open conversation."),
    ("sidebar", "Places, labels and saved searches, on the left."),
    (
        "place",
        "Newsletters and automated mail, bundled by sender, opened from the sidebar.",
    ),
    (
        "now",
        "The front page the TUI opens on. Acting on a row does it in that row's own mode.",
    ),
    (
        "messages",
        "People as rows with their topics inside. See [Messages](/guides/messages/).",
    ),
    ("todo", "See [Act on To dos](/guides/todo/)."),
    (
        "catchup",
        "The first run's older undated tasks: keep each one, or let it go to the Expired list.",
    ),
    ("expired", "Tasks that expired or were let go."),
    (
        "updates",
        "The briefing: the latest digest, one line per source, with what arrived since below the cut.",
    ),
    (
        "reading",
        "The [edition](/guides/reading/). Mail keys such as archive, star and reply don't apply here.",
    ),
    ("reading-reader", "One item from the edition, open."),
    ("archive", "The records lens, with the answer box focused."),
    (
        "focus",
        "`g F`: everyone you owe a reply, one conversation at a time. `r` writes the reply in `$EDITOR`, and Send in the review moves on to the next. The navigation keys above work here too.",
    ),
    ("screener", "New senders waiting for a decision."),
    (
        "invites",
        "The calendar invites list, from the sidebar. The `i` keys above answer an invite.",
    ),
    ("search", "The Search screen's results."),
    ("search-preview", "The thread previewed beside the results."),
    ("search-editor", "While you type the query. Letters type."),
    ("rules", ""),
    ("rules-form", "Letters type into the field; these keys don't."),
    ("accounts", "Accounts, and the TUI's settings."),
    (
        "accounts-form",
        "While a field is being edited, letters type into it, and `Enter` or `Esc` stops editing.",
    ),
    ("diagnostics", ""),
    ("analytics", "These work in every view."),
    ("analytics-storage", ""),
    ("analytics-stale", ""),
    ("analytics-contacts", ""),
    ("analytics-response-time", ""),
    ("analytics-subscriptions", ""),
    ("analytics-wrapped", "The year in review moves between tiles instead of rows."),
    (
        "deliveries",
        "[Tracked packages](/guides/deliveries/) detected in your mail.",
    ),
    (
        "assistant",
        "`@` opens the pane and gives it the keys. While it has them, nothing you type reaches the screen behind it. See [the assistant guide](/guides/assistant/#in-the-terminal).",
    ),
    ("assistant-input", ""),
    (
        "compose-assist-proof",
        "The proof shown for a draft written in your voice, before it goes into your editor.",
    ),
];

/// What the overlays' tables say before them.
const OVERLAYS_INTRO: &str = "An open overlay takes every key until it closes. Each one closes with `Esc` without changing anything, and with `q` where nothing is typed; `Enter` runs the highlighted option, and in a yes-or-no question `y` is `Enter` and `n` is `Esc`. Each prints its keys in its footer too.";

/// Each TUI overlay's table, in page order, with what to say above it.
pub(crate) const OVERLAYS: &[(&str, &str)] = &[
    ("help", "`?` opens it. Letters type into its filter."),
    ("palette", "`:` or `Ctrl-k` opens it. Letters type into its filter."),
    ("label-picker", "`l` labels and `v` moves. Letters type into its filter."),
    ("links", "`L` opens it."),
    ("unsubscribe", "`D` opens it."),
    (
        "move-picker",
        "`X` moves this email; a capital moves everything from its sender.",
    ),
    ("move-picker-sender", "`P` moves everything from the sender. Either case picks."),
    (
        "new-sender-picker",
        "`X` or `P` on a new sender: a capital answers its question (from `P`, either case does).",
    ),
    ("sender-kind-picker", "`P` in Reading and Paper trail. Either case picks."),
    ("not-now", "`b` opens it on Later; the digits pick the presets."),
    (
        "not-now-custom",
        "Letters type the time, read as you type.",
    ),
    ("correct-todo", "`X` on a to-do."),
    (
        "delete-confirm",
        "Deleting a saved search, rule, category, draft or assistant conversation, and cancelling a scheduled send. It opens on Keep, so a bare `Enter` changes nothing.",
    ),
    ("bulk-confirm", "A batch action over many conversations shows them first."),
    ("sweep-confirm", "`E` in Reading and Paper trail. A whole place's sweep opens on Cancel."),
    (
        "reply-later",
        "Letters type when to bring it back; with nothing typed, `Enter` flags it for reply later.",
    ),
    ("sender-ask", "After a Not-sure answer. Any other key drops the question and does what it always does."),
    ("categories", ""),
    (
        "assistant-approval",
        "A change the assistant wants to make, waiting in its pane. Any other key works as it does in the pane.",
    ),
    (
        "assistant-question",
        "A question the assistant asks, waiting in its pane. Any other key works as it does in the pane.",
    ),
    ("assistant-history", "`L` in the assistant pane opens it."),
    (
        "send-review",
        "After you save and quit `$EDITOR`. It opens on Send.",
    ),
    ("send-time", "Letters type the time, read as you type."),
    ("drafts", ""),
    ("compose-picker", "`c` opens it. Letters type into its fields."),
    ("model-picker", ""),
    ("draft-options", "Letters type into the instruction field."),
    ("voice-source", ""),
    ("attachments", "`A` opens it."),
    ("save-attachment", "Letters type the path."),
    ("export", "Export, from the palette."),
    ("export-destination", ""),
    ("export-path", "Letters type the path."),
    ("filter-bar", "`Ctrl-f` or `g f` opens it. Letters type the filter."),
    ("messages-filter", "`g f` or `Ctrl-f` in Messages."),
    ("saved-search-form", "Letters type into the field."),
    ("analytics-filter", "Letters type into the field."),
    ("todo-prompt", "Letters type the to-do, or `field=value` to fix one."),
    ("records-prompt", "Letters type `field=value`."),
    ("pass-menu", "Pass to a mode, from the palette."),
    ("arrivals-list", "The emails behind Now's arrivals line."),
    ("snippets", ""),
    ("sender-profile", ""),
    ("reply-queue", ""),
    ("sheet", ""),
    ("platform", ""),
    ("error", ""),
    ("welcome", "On first run, before an account exists."),
    ("resume-draft", "Adding an account while an unsaved account form is kept."),
    ("account-mode-switch", "Changing an account form's type drops what doesn't fit the new one."),
    ("identity-editor", "`i` on the Accounts screen. Letters type the tag."),
    ("auth-session", "While Microsoft sign-in waits for the code."),
    ("promise-prompt", "Any other key works as usual and leaves the question for later."),
];

impl Keymap {
    /// Markdown tables of every TUI key, by place: the keys every mail
    /// context shares, then the list's and reader's, then each place's own.
    pub fn tui_reference_markdown(&self) -> String {
        let everywhere = self.shared_keys(&mail_contexts().collect::<Vec<_>>(), &BTreeSet::new());
        let on_an_email = self.shared_keys(&ON_AN_EMAIL, &everywhere);
        let mut out = Vec::new();
        section(
            &mut out,
            "###",
            "Everywhere in mail",
            "In the mail list, the reader, the sidebar and every mode.",
            self.place_rows("list", |action, key| {
                everywhere.contains(&(action.id.clone(), key.to_string()))
            }),
        );
        section(
            &mut out,
            "###",
            "List and reader",
            "In the mail list, the reader, and the Reading and Paper trail lists. Mail verbs act on the selected conversations, or on the open one.",
            self.place_rows("list", |action, key| {
                on_an_email.contains(&(action.id.clone(), key.to_string()))
            }),
        );
        for (place, intro) in PLACES {
            let skip_email_verbs = ON_AN_EMAIL.contains(place);
            let rows = self.place_rows(place, |action, key| {
                let row = (action.id.clone(), key.to_string());
                !everywhere.contains(&row) && !(skip_email_verbs && on_an_email.contains(&row))
            });
            let title = place_title(place).unwrap_or(place);
            section(&mut out, "###", title, intro, rows);
        }
        let overlays: Vec<_> = OVERLAYS
            .iter()
            .map(|(place, intro)| (place, intro, self.place_rows(place, |_, _| true)))
            .filter(|(_, _, rows)| !rows.is_empty())
            .collect();
        if !overlays.is_empty() {
            out.push("### Overlays".into());
            out.push(String::new());
            out.push(OVERLAYS_INTRO.into());
            out.push(String::new());
            for (place, intro, rows) in overlays {
                let title = place_title(place).unwrap_or(place);
                section(&mut out, "####", title, intro, rows);
            }
        }
        out.join("\n").trim_end().to_string()
    }

    /// `(action id, key)` pairs bound in every one of `places`.
    fn shared_keys(
        &self,
        places: &[&str],
        except: &BTreeSet<(String, String)>,
    ) -> BTreeSet<(String, String)> {
        let mut shared = BTreeSet::new();
        for action in &self.actions {
            for key in action.tui.iter().flat_map(|binding| &binding.keys) {
                let everywhere = places.iter().all(|place| {
                    action
                        .tui
                        .iter()
                        .any(|b| b.contexts.includes(place) && b.keys.contains(key))
                });
                let row = (action.id.clone(), key.clone());
                if everywhere && !except.contains(&row) {
                    shared.insert(row);
                }
            }
        }
        shared
    }

    /// One row per action with keys in `place` that `keep` accepts, in the
    /// definition's order: the keys cell, then the label.
    fn place_rows(
        &self,
        place: &str,
        keep: impl Fn(&KeyAction, &str) -> bool,
    ) -> Vec<(String, String)> {
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut saved_searches = Vec::new();
        for action in &self.actions {
            let mut keys: Vec<String> = Vec::new();
            for binding in action.tui.iter().filter(|b| b.contexts.includes(place)) {
                for key in binding.keys.iter().filter(|key| keep(action, key)) {
                    if !keys.contains(key) {
                        keys.push(key.clone());
                    }
                }
            }
            if keys.is_empty() {
                continue;
            }
            // Nine rows that differ only by their digit read better as one.
            if action.id.starts_with("nav.saved-search-") {
                saved_searches.extend(keys);
                continue;
            }
            let cell = keys_cell(&keys);
            rows.push((cell, action.label.clone()));
        }
        if let (Some(first), Some(last)) = (saved_searches.first(), saved_searches.last()) {
            rows.push((
                format!("{} … {}", code(first), code(last)),
                format!(
                    "Open saved search 1 to {}, in sidebar order",
                    saved_searches.len()
                ),
            ));
        }
        rows
    }
}

fn section(
    out: &mut Vec<String>,
    level: &str,
    title: &str,
    intro: &str,
    rows: Vec<(String, String)>,
) {
    if rows.is_empty() {
        return;
    }
    out.push(format!("{level} {title}"));
    out.push(String::new());
    if !intro.is_empty() {
        out.push(intro.to_string());
        out.push(String::new());
    }
    out.push("| Key | Action |".into());
    out.push("|-----|--------|".into());
    for (keys, label) in rows {
        out.push(format!("| {keys} | {} |", escape_cell(&label)));
    }
    out.push(String::new());
}

/// The keys, `, ` between them; a run of three or more digits reads as one
/// range (`1`–`9`).
fn keys_cell(keys: &[String]) -> String {
    let digit = |key: &String| key.len() == 1 && key.as_bytes()[0].is_ascii_digit();
    let consecutive = keys.windows(2).all(|pair| {
        pair[1].as_bytes().first().copied() == pair[0].as_bytes().first().map(|b| b + 1)
    });
    if keys.len() > 2 && keys.iter().all(digit) && consecutive {
        return format!("{}–{}", code(&keys[0]), code(&keys[keys.len() - 1]));
    }
    keys.iter()
        .map(|key| code(key))
        .collect::<Vec<_>>()
        .join(", ")
}

fn code(key: &str) -> String {
    format!("`{}`", escape_cell(&doc_key(key)))
}

/// A key as the page spells it: the definition's spelling, with arrows
/// and Esc as people see them on the key.
fn doc_key(key: &str) -> String {
    key.split(' ')
        .map(|chord| match chord {
            "Escape" => "Esc",
            "ArrowUp" => "↑",
            "ArrowDown" => "↓",
            "ArrowLeft" => "←",
            "ArrowRight" => "→",
            chord => chord,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|")
}
