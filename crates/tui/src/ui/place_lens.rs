//! Reading and Paper trail: mail that isn't from people, bundled by sender.
//!
//! Renders `Request::ListPlace` as one header per sender (name, count, and
//! why the sender is here) with the bundle's messages under it; one cursor
//! walks the messages. Nothing is styled or counted as unread. Also draws
//! the sweep preview and the "move sender to…" menu. Pure render; wiring
//! lives in `app/place_actions.rs`.

use mxr_protocol::{
    AccountSummaryData, MailPlaceData, PlaceBundleData, PlaceMessageData, SenderKindData,
};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, PendingSweepConfirm, PlacePageState, SenderKindMenu};
use crate::ui::account_mark::AccountMarks;
use crate::ui::centered_rect_fixed_height;
use crate::ui::sanitize::{one_line, truncate};

pub struct PlaceView<'a> {
    pub page: &'a PlacePageState,
    pub place: MailPlaceData,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
    /// The zone times show in: `Local` in the app, a fixed one in tests.
    pub zone: crate::ui::received_label::DisplayZone,
    pub account_marks: &'a AccountMarks,
}

pub(crate) const fn place_title(place: MailPlaceData) -> &'static str {
    match place {
        MailPlaceData::Reading => "Reading",
        MailPlaceData::PaperTrail => "Updates",
    }
}

/// What each place's mode is built on for now: both are early versions.
const fn early_note(place: MailPlaceData) -> &'static str {
    match place {
        MailPlaceData::Reading => "early version: newsletters and lists in your inbox by sender",
        MailPlaceData::PaperTrail => "early version: automated mail in your inbox by sender",
    }
}

const FOOTER: &str = "  enter open \u{b7} e done here \u{b7} s pin \u{b7} P move sender \u{b7} E sweep all \u{b7} u undo \u{b7} : sweep one sender";

pub fn draw(frame: &mut Frame, area: Rect, view: &PlaceView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let block = Block::bordered()
        .title(format!(
            " {} \u{2500} {} ",
            place_title(view.place),
            early_note(view.place)
        ))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_line = 0usize;
    let mut row_index = 0usize;
    let page = view.page;

    if !page.loaded {
        lines.push(Line::from(Span::styled(
            format!("  Loading {}…", place_title(view.place)),
            Style::default().fg(theme.text_muted),
        )));
    } else if page.bundles.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {} is clear.", place_title(view.place)),
            Style::default().fg(theme.text_secondary),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "  {} from {}",
                plural(page.total_messages, "message", "messages"),
                plural(page.total_bundles, "sender", "senders"),
            ),
            Style::default().fg(theme.text_muted),
        )));
    }

    let more_senders = page.total_bundles as usize > page.bundles.len();

    for bundle in &page.bundles {
        lines.push(Line::from(""));
        lines.push(bundle_header(bundle, width, theme));
        lines.push(Line::from(Span::styled(
            format!("    here because: {}", one_line(&bundle.kind.reason)),
            Style::default().fg(theme.text_muted),
        )));
        let chip = view
            .account_marks
            .row_cell(&bundle.account_id, theme)
            .unwrap_or_default();
        for message in &bundle.messages {
            let selected = row_index == view.selected_index;
            if selected {
                selected_line = lines.len();
            }
            lines.push(message_line(
                message, selected, width, &chip, theme, view.zone,
            ));
            row_index += 1;
        }
        let hidden = (bundle.message_count as usize).saturating_sub(bundle.messages.len());
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                format!("      and {hidden} more (palette: More from this sender)"),
                Style::default().fg(theme.text_muted),
            )));
        }
    }

    if more_senders {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "  {} more senders (palette: More senders in this place)",
                page.total_bundles as usize - page.bundles.len()
            ),
            Style::default().fg(theme.text_muted),
        )));
    }

    let footer_height = 1u16;
    let body_height = inner.height.saturating_sub(footer_height) as usize;
    let scroll = selected_line.saturating_sub(body_height.saturating_sub(2));
    let body_area = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(footer_height),
        height: footer_height,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            FOOTER,
            Style::default().fg(theme.text_muted),
        ))),
        footer_area,
    );
}

fn bundle_header(
    bundle: &PlaceBundleData,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let who = one_line(
        bundle
            .sender_name
            .as_deref()
            .unwrap_or(&bundle.sender_email),
    );
    let count = bundle.message_count.to_string();
    let pinned = if bundle.pinned_count > 0 {
        format!(" \u{b7} {} pinned", bundle.pinned_count)
    } else {
        String::new()
    };
    let who_width = width.saturating_sub(4 + count.len() + pinned.len()).max(8);
    Line::from(vec![
        Span::styled(
            format!("  {}", truncate(&who, who_width)),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {count}"), Style::default().fg(theme.text_muted)),
        Span::styled(pinned, Style::default().fg(theme.accent)),
    ])
}

/// One message. `chip` is the bundle's account chip, empty when the view
/// shows none; it sits in the trailing column before the date.
fn message_line(
    message: &PlaceMessageData,
    selected: bool,
    width: usize,
    chip: &[Span<'static>],
    theme: &crate::theme::Theme,
    zone: crate::ui::received_label::DisplayZone,
) -> Line<'static> {
    let marker = if selected { "\u{258c} " } else { "  " };
    let pin = if message.pinned { "\u{2022} " } else { "  " };
    let date = zone.at(message.date).format("%-d %b").to_string();
    let chip_width: usize = chip.iter().map(Span::width).sum();
    // marker (2) + indent (2) + pin (2) + subject + gap (2) + chip + date
    let subject_width = width
        .saturating_sub(2 + 2 + 2 + 2 + chip_width + date.chars().count())
        .max(8);
    let mut spans = vec![
        Span::raw(marker),
        Span::raw("  "),
        Span::styled(pin, Style::default().fg(theme.accent)),
        Span::styled(
            format!(
                "{:<subject_width$}  ",
                truncate(&one_line(&message.subject), subject_width)
            ),
            Style::default().fg(theme.text_secondary),
        ),
    ];
    spans.extend(chip.iter().cloned());
    spans.push(Span::styled(date, Style::default().fg(theme.text_muted)));
    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
    Line::from(spans)
}

/// The sweep preview: what the daemon's dry run says will be archived.
pub fn draw_sweep_confirm(
    frame: &mut Frame,
    area: Rect,
    confirm: Option<&PendingSweepConfirm>,
    accounts: &[AccountSummaryData],
    theme: &crate::theme::Theme,
) {
    let Some(confirm) = confirm else {
        return;
    };
    let preview = &confirm.preview;
    // Sized to its content: a count, up to five subjects, the keys.
    let height = u16::try_from(preview.sample_subjects.len()).unwrap_or(5) + 10;
    let popup = centered_rect_fixed_height(64, height, area);
    frame.render_widget(Clear, popup);
    let scope = match &confirm.target.sender_email {
        Some(sender) => format!(
            "{sender} in {}{}",
            place_title(confirm.target.place),
            if confirm.from_updates {
                " · Updates"
            } else {
                ""
            }
        ),
        None => format!("all of {}", place_title(confirm.target.place)),
    };
    let block = Block::bordered()
        .title(format!(" Sweep {} ", one_line(&scope)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // A sweep reaches mail the lens has not loaded yet: say how much.
    let shown = if !confirm.from_updates && confirm.shown < preview.count {
        format!(" ({} shown here)", confirm.shown)
    } else {
        String::new()
    };
    let mut lines = vec![Line::from(format!(
        "Archive {}{shown}{}.",
        plural(preview.count, "message", "messages"),
        if preview.pinned_excluded > 0 {
            format!(
                "; {} stays",
                plural(preview.pinned_excluded, "pinned message", "pinned messages")
            )
        } else {
            String::new()
        }
    ))];
    let subject_width = (inner.width as usize).saturating_sub(2);
    let account_prefix = "Account: ";
    let account_width = (inner.width as usize).saturating_sub(Line::from(account_prefix).width());
    let account = match confirm.target.account_id.as_ref() {
        Some(account_id) => accounts
            .iter()
            .find(|account| &account.account_id == account_id)
            .map_or_else(
                || "unknown account".into(),
                |account| {
                    let name = one_line(&account.name);
                    let email = one_line(&account.email);
                    let identity = if name.trim().is_empty() || name == email {
                        email.clone()
                    } else {
                        format!("{name} <{email}>")
                    };
                    // Keep a useful address visible when a configured account
                    // name would otherwise consume the whole line.
                    if Line::from(identity.as_str()).width() > account_width {
                        email
                    } else {
                        identity
                    }
                },
            ),
        None => "all accounts".into(),
    };
    lines.push(Line::from(format!(
        "{account_prefix}{}",
        truncate(&account, account_width)
    )));
    if confirm.target.sender_email.is_none() {
        lines.push(Line::from(Span::styled(
            format!(
                "From {}",
                plural(preview.senders.len() as u32, "sender", "senders")
            ),
            Style::default().fg(theme.text_muted),
        )));
    }
    lines.push(Line::from(""));
    for subject in &preview.sample_subjects {
        lines.push(Line::from(Span::styled(
            format!("  {}", truncate(&one_line(subject), subject_width)),
            Style::default().fg(theme.text_secondary),
        )));
    }
    lines.push(Line::from(""));
    let button = |label: String, focused: bool| {
        let style = if focused {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_secondary)
        };
        Span::styled(format!("[ {label} ]"), style)
    };
    lines.push(Line::from(vec![
        button("Cancel".into(), !confirm.sweep_focused),
        Span::raw("  "),
        button(sweep_confirm_label(confirm), confirm.sweep_focused),
    ]));
    lines.push(Line::from(Span::styled(
        "Tab switches, Enter chooses, Esc cancels; u undoes a sweep",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The confirm button names the scope, so the whole place never reads
/// like one sender's bundle: "Archive all 143 from 35 senders".
fn sweep_confirm_label(confirm: &PendingSweepConfirm) -> String {
    let preview = &confirm.preview;
    if confirm.target.sender_email.is_some() {
        return format!("Archive {}", plural(preview.count, "message", "messages"));
    }
    format!(
        "Archive all {} from {}",
        preview.count,
        plural(preview.senders.len() as u32, "sender", "senders")
    )
}

/// The "move sender to…" menu. Its keys: p people, r Reading, t Paper
/// trail, x screened out, a automatic.
/// The sender-kind menu's options, on the `g` letters of the modes they
/// feed (D168): People are Messages' `m`, Paper trail is Updates' `u`, and
/// Screened out takes `d`, chapter 22's screen-out letter.
pub const SENDER_KIND_CHOICES: [(char, Option<SenderKindData>, &str); 5] = [
    ('m', Some(SenderKindData::People), "People: on the desk"),
    ('r', Some(SenderKindData::Reading), "Reading"),
    ('u', Some(SenderKindData::PaperTrail), "Paper trail"),
    ('d', Some(SenderKindData::ScreenedOut), "Screened out"),
    ('a', None, "Automatic: let the rules decide"),
];

pub fn draw_sender_kind_menu(
    frame: &mut Frame,
    area: Rect,
    menu: Option<&SenderKindMenu>,
    theme: &crate::theme::Theme,
) {
    let Some(menu) = menu else {
        return;
    };
    let popup = centered_rect_fixed_height(52, 14, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Move sender to… ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = vec![
        Line::from(Span::styled(
            truncate(&one_line(&menu.display), 46),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("here because: {}", one_line(&menu.current.reason)),
            Style::default().fg(theme.text_muted),
        )),
        Line::from(""),
    ];
    for (index, (key, kind, label)) in SENDER_KIND_CHOICES.into_iter().enumerate() {
        let current = kind == Some(menu.current.kind) && menu.current.corrected
            || kind.is_none() && !menu.current.corrected;
        let lit = index == menu.selected;
        lines.push(Line::from(vec![
            Span::raw(if lit { "\u{203a} " } else { "  " }),
            Span::styled(format!("{key}  "), Style::default().fg(theme.accent)),
            Span::styled(
                label,
                if lit {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                if current { "  (now)" } else { "" },
                Style::default().fg(theme.text_muted),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Remembered for this sender's future mail. j/k move, Enter picks, Esc closes.",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::SweepTarget;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{KindRuleData, MailKindData, SweepPreviewData};
    use mxr_test_support::render_to_string;

    fn bundle(name: &str, subjects: &[&str], pinned: usize) -> PlaceBundleData {
        PlaceBundleData {
            account_id: AccountId::new(),
            sender_email: format!("{}@shop.example", name.to_lowercase()),
            sender_name: Some(name.into()),
            kind: MailKindData {
                kind: SenderKindData::PaperTrail,
                rule: KindRuleData::AutomatedAddress,
                reason: "automated sender, has List-Unsubscribe".into(),
                corrected: false,
            },
            message_count: subjects.len() as u32 + 2,
            unread_count: 3,
            pinned_count: u32::from(pinned > 0),
            newest_at: chrono::Utc::now(),
            newest_subject: subjects[0].into(),
            messages: subjects
                .iter()
                .enumerate()
                .map(|(index, subject)| PlaceMessageData {
                    message_id: MessageId::new(),
                    thread_id: ThreadId::new(),
                    subject: (*subject).into(),
                    snippet: String::new(),
                    date: chrono::Utc::now(),
                    unread: true,
                    pinned: index < pinned,
                    starred: false,
                })
                .collect(),
        }
    }

    fn page(bundles: Vec<PlaceBundleData>) -> PlacePageState {
        PlacePageState {
            place: Some(MailPlaceData::PaperTrail),
            total_bundles: bundles.len() as u32,
            total_messages: bundles.iter().map(|b| b.message_count).sum(),
            bundles,
            loaded: true,
        }
    }

    fn render(page: &PlacePageState) -> String {
        render_to_string(100, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 20),
                &PlaceView {
                    zone: crate::ui::received_label::DisplayZone::Fixed(
                        chrono::FixedOffset::east_opt(3600).unwrap(),
                    ),
                    page,
                    place: MailPlaceData::PaperTrail,
                    selected_index: 1,
                    active_pane: &ActivePane::MailList,
                    account_marks: &AccountMarks::default(),
                },
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn mixed_accounts_mark_each_message_with_its_senders_chip() {
        use chrono::TimeZone;
        let work = AccountId::new();
        let home = AccountId::new();
        let mut shop = bundle("Shop", &["Receipt 2"], 0);
        shop.account_id = work.clone();
        let mut bank = bundle("Bank", &["Statement 1"], 0);
        bank.account_id = home.clone();
        // A fixed date keeps the snapshot the same tomorrow.
        let fixed = chrono::Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap();
        for message in shop.messages.iter_mut().chain(bank.messages.iter_mut()) {
            message.date = fixed;
        }
        let marks = crate::ui::account_mark::plain_marks_for_tests(&[
            (&work, "Work", "W"),
            (&home, "Home", "H"),
        ]);
        let page = page(vec![shop, bank]);
        let rendered = render_to_string(100, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 20),
                &PlaceView {
                    zone: crate::ui::received_label::DisplayZone::Fixed(
                        chrono::FixedOffset::east_opt(3600).unwrap(),
                    ),
                    page: &page,
                    place: MailPlaceData::PaperTrail,
                    selected_index: 0,
                    active_pane: &ActivePane::MailList,
                    account_marks: &marks,
                },
                &crate::theme::Theme::default(),
            );
        });
        let receipt = rendered
            .lines()
            .find(|line| line.contains("Receipt 2"))
            .expect("the Shop message");
        let statement = rendered
            .lines()
            .find(|line| line.contains("Statement 1"))
            .expect("the Bank message");
        assert!(receipt.contains("[W]"), "{rendered}");
        assert!(statement.contains("[H]"), "{rendered}");
        insta::assert_snapshot!("place_lens_mixed_accounts", rendered);
    }

    #[test]
    fn bundles_say_who_how_many_and_why_without_unread_counts() {
        let rendered = render(&page(vec![bundle("Shop", &["Receipt 2", "Receipt 1"], 1)]));
        assert!(rendered.contains("Updates"), "{rendered}");
        assert!(rendered.contains("early version"), "{rendered}");
        assert!(rendered.contains("4 messages from 1 sender"));
        assert!(rendered.contains("Shop  4 \u{b7} 1 pinned"));
        assert!(rendered.contains("here because: automated sender, has List-Unsubscribe"));
        assert!(rendered.contains("\u{2022} Receipt 2"), "pinned marker");
        assert!(rendered.contains("and 2 more (palette: More from this sender)"));
        assert!(!rendered.to_lowercase().contains("unread"));
        assert!(!rendered.contains('\u{2014}'), "no em dashes");
    }

    #[test]
    fn an_empty_place_is_clear_and_mail_text_is_sanitised() {
        assert!(render(&page(vec![])).contains("Updates is clear."));
        let hostile = bundle("Eve\u{1b}]0;pwned\u{7}", &["Invoice\u{1b}[2J\r\nnow"], 0);
        let rendered = render(&page(vec![hostile]));
        assert!(!rendered.chars().any(|c| c == '\u{1b}' || c == '\u{7}'));
    }

    #[test]
    fn the_sweep_preview_names_the_count_and_the_pins_that_stay() {
        let confirm = PendingSweepConfirm {
            target: SweepTarget {
                place: MailPlaceData::PaperTrail,
                account_id: None,
                sender_email: Some("receipts@shop.example".into()),
            },
            preview: SweepPreviewData {
                place: MailPlaceData::PaperTrail,
                sender_email: Some("receipts@shop.example".into()),
                count: 13,
                pinned_excluded: 1,
                senders: vec![],
                sample_subjects: vec!["Receipt 14".into()],
                preview_token: Some("tok".into()),
            },
            shown: 1,
            sweep_focused: true,
            from_updates: false,
        };
        let rendered = render_to_string(100, 30, |frame| {
            draw_sweep_confirm(
                frame,
                Rect::new(0, 0, 100, 30),
                Some(&confirm),
                &[],
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            rendered.contains("Archive 13 messages (1 shown here); 1 pinned message stays."),
            "{rendered}"
        );
        assert!(rendered.contains("Receipt 14"));
    }

    #[test]
    fn the_sweep_confirmation_keeps_account_identity_visible_at_80_columns() {
        let account = mxr_protocol::AccountSummaryData {
            identity: None,
            account_id: AccountId::new(),
            key: Some("work".into()),
            name: "A deliberately long configured account label that should not hide the address"
                .into(),
            email: "work+updates+alerts@very-long-company.example".into(),
            provider_kind: "gmail".into(),
            sync_kind: Some("gmail".into()),
            send_kind: Some("gmail".into()),
            enabled: true,
            is_default: false,
            source: mxr_protocol::AccountSourceData::Config,
            editable: mxr_protocol::AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
        };
        let mut confirm = PendingSweepConfirm {
            target: SweepTarget {
                place: MailPlaceData::PaperTrail,
                account_id: Some(account.account_id.clone()),
                sender_email: Some("a-very-long-automated-sender-name@shop.example".into()),
            },
            preview: SweepPreviewData {
                place: MailPlaceData::PaperTrail,
                sender_email: Some("a-very-long-automated-sender-name@shop.example".into()),
                count: 13,
                pinned_excluded: 1,
                senders: vec![],
                sample_subjects: vec![
                    "Receipt 11: quarterly statement with reference 394817 and account details"
                        .into(),
                    "Receipt 12: quarterly statement with reference 394818 and account details"
                        .into(),
                    "Receipt 13: quarterly statement with reference 394819 and account details"
                        .into(),
                    "Receipt 14: quarterly statement with reference 394820 and account details"
                        .into(),
                    "Receipt 15: quarterly statement with reference 394821 and account details"
                        .into(),
                ],
                preview_token: Some("tok".into()),
            },
            shown: 1,
            sweep_focused: true,
            from_updates: true,
        };
        let render = |confirm: &PendingSweepConfirm,
                      accounts: &[mxr_protocol::AccountSummaryData]| {
            render_to_string(80, 24, |frame| {
                draw_sweep_confirm(
                    frame,
                    Rect::new(0, 0, 80, 24),
                    Some(confirm),
                    accounts,
                    &crate::theme::Theme::default(),
                );
            })
        };

        let rendered = render(&confirm, std::slice::from_ref(&account));
        let account_line = rendered
            .lines()
            .find(|line| line.contains("Account:"))
            .expect("account line is rendered");
        assert!(
            account_line.contains("Account: work+updates+alerts@very-long"),
            "{rendered}"
        );
        assert!(
            account_line.trim_end_matches('"').trim_end().ends_with('│'),
            "{rendered}"
        );
        assert!(
            rendered.contains("Archive 13 messages; 1 pinned message stays."),
            "{rendered}"
        );
        let subject_rows: Vec<_> = rendered
            .lines()
            .filter(|line| line.contains("Receipt "))
            .collect();
        assert_eq!(subject_rows.len(), 5, "{rendered}");
        assert!(
            subject_rows
                .iter()
                .all(|line| line.trim_end_matches('"').trim_end().ends_with('│')),
            "{rendered}"
        );
        assert!(rendered.contains("Cancel"), "{rendered}");
        assert!(rendered.contains("Archive 13 messages"), "{rendered}");
        assert!(
            rendered.contains("Tab switches, Enter chooses"),
            "{rendered}"
        );
        assert!(rendered.contains("Esc cancels; u undoes"), "{rendered}");

        confirm.target.account_id = None;
        confirm.target.sender_email = None;
        confirm.preview.sender_email = None;
        confirm.preview.senders = vec![
            mxr_protocol::SweepSenderData {
                account_id: AccountId::new(),
                sender_email: "one@example.com".into(),
                sender_name: None,
                count: 1,
            },
            mxr_protocol::SweepSenderData {
                account_id: AccountId::new(),
                sender_email: "two@example.com".into(),
                sender_name: None,
                count: 1,
            },
        ];
        let all_senders_rendered = render(&confirm, &[]);
        assert!(
            all_senders_rendered.contains("Account: all accounts"),
            "{all_senders_rendered}"
        );
        assert!(
            all_senders_rendered.contains("From 2 senders"),
            "{all_senders_rendered}"
        );
        assert!(
            all_senders_rendered.contains("Archive 13 messages; 1 pinned message stays."),
            "{all_senders_rendered}"
        );
        assert!(
            all_senders_rendered.contains("Archive all 13 from 2 senders"),
            "{all_senders_rendered}"
        );
        assert!(
            all_senders_rendered.contains("Cancel"),
            "{all_senders_rendered}"
        );
        assert!(
            all_senders_rendered.contains("Tab switches, Enter chooses"),
            "{all_senders_rendered}"
        );
        assert!(
            all_senders_rendered.contains("Esc cancels; u undoes"),
            "{all_senders_rendered}"
        );

        confirm.target.sender_email = Some("receipts@shop.example".into());
        confirm.preview.sender_email = Some("receipts@shop.example".into());
        confirm.target.account_id = Some(AccountId::new());
        assert!(render(&confirm, &[]).contains("Account: unknown account"));
    }
}
