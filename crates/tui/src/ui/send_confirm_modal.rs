use super::centered_rect_fixed_height;
use crate::app::{PendingSend, PendingSendMode};
use mxr_core::{DraftSafetyReport, DraftSafetySeverity, DraftSafetyVerdict};
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PCT: u16 = 86;

#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    pending: Option<&PendingSend>,
    send_at_input: Option<&str>,
    remind_at_input: Option<&str>,
    time_preview: &[String],
    theme: &crate::theme::Theme,
    account_marks: &crate::ui::account_mark::AccountMarks,
) {
    let Some(pending) = pending else {
        return;
    };

    let lines = modal_lines(pending, send_at_input, remind_at_input, time_preview);
    let inner_width = (u32::from(area.width) * u32::from(MODAL_WIDTH_PCT) / 100)
        .saturating_sub(2)
        .max(1) as usize;
    let content_rows: u16 = lines
        .iter()
        .map(|line| wrapped_row_count(line, inner_width))
        .sum();
    let popup_height = content_rows.saturating_add(2).min(area.height);
    let popup = centered_rect_fixed_height(MODAL_WIDTH_PCT, popup_height, area);
    frame.render_widget(Clear, popup);

    let border_color = match pending.safety_report.as_ref().map(|r| r.verdict) {
        Some(DraftSafetyVerdict::Blocked) => theme.error,
        Some(DraftSafetyVerdict::Warn) => theme.warning,
        _ => theme.warning,
    };

    let block = Block::bordered()
        .title(" Draft Ready ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut paragraph_lines = lines
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect::<Vec<_>>();
    if let (Some(index), Some(chip)) = (
        lines.iter().position(|line| line.starts_with("From: ")),
        account_marks.chip(&pending.account_id, theme),
    ) {
        let mut spans = chip;
        spans.push(Span::raw(" "));
        spans.push(Span::raw(lines[index].clone()));
        paragraph_lines[index] = Line::from(spans);
    }
    frame.render_widget(
        Paragraph::new(paragraph_lines).wrap(Wrap { trim: false }),
        inner,
    );
}

fn wrapped_row_count(line: &str, width: usize) -> u16 {
    if width == 0 {
        return 1;
    }
    let mut rows: u16 = 1;
    let mut col = 0usize;
    for word in line.split_whitespace() {
        let word_len = word.chars().count();
        if col == 0 {
            col = word_len;
        } else if col + 1 + word_len <= width {
            col += 1 + word_len;
        } else {
            rows = rows.saturating_add(1);
            col = word_len;
        }
        while col > width {
            rows = rows.saturating_add(1);
            col -= width;
        }
    }
    rows.max(1)
}

fn modal_lines(
    pending: &PendingSend,
    send_at_input: Option<&str>,
    remind_at_input: Option<&str>,
    time_preview: &[String],
) -> Vec<String> {
    let mut lines = vec![match pending.mode {
        PendingSendMode::SendOrSave => "Send this draft?".to_string(),
        PendingSendMode::DraftOnlyNoRecipients => "No recipients yet. Save as draft?".to_string(),
        PendingSendMode::Unchanged => "Draft unchanged. Discard or keep editing?".to_string(),
    }];

    if let Some(address) = pending.resolved_from.as_ref() {
        let rendered = match address.name.as_deref() {
            Some(name) if !name.trim().is_empty() => format!("{name} <{}>", address.email),
            _ => address.email.clone(),
        };
        lines.push(format!("From: {rendered}"));
    } else if !pending.fm.from.trim().is_empty() {
        lines.push(format!("From: {}", pending.fm.from));
    }
    lines.push(format!("Subject: {}", pending.fm.subject));
    lines.push("Voice match: not scored for manual edits".to_string());
    lines.push("Humanizer: scored on AI draft outputs".to_string());

    if let Some(report) = pending.safety_report.as_ref() {
        lines.push(String::new());
        push_safety_lines(&mut lines, report, pending.override_token.as_deref());
    } else if let Some(reason) = pending.safety_check_failed.as_deref() {
        lines.push(String::new());
        lines.push(format!("Safety check unavailable: {reason}"));
    }

    if !pending.suggested_collaborators.is_empty() {
        lines.push(String::new());
        lines.push("Maybe include:".to_string());
        for (i, s) in pending.suggested_collaborators.iter().enumerate() {
            let display = s.display_name.as_deref().unwrap_or(s.email.as_str());
            lines.push(format!(
                "  {}. {display} ({} threads)",
                i + 1,
                s.evidence_msg_ids.len()
            ));
        }
        lines.push(format!(
            "Press {} to add the top suggestion to Cc.",
            mxr_keymap::guide_key("send-review.add-cc").unwrap_or_default()
        ));
    }

    if let Some(input) = send_at_input {
        lines.push(format!("Send at: {input}"));
        lines.extend(time_preview.iter().cloned());
        lines.push("Enter to schedule. Esc to cancel prompt.".to_string());
    }
    if let Some(input) = remind_at_input {
        lines.push(format!("Remind at: {input}"));
        lines.extend(time_preview.iter().cloned());
        lines.push("Enter to send + set reminder. Esc to cancel prompt.".to_string());
    }
    lines.push(String::new());

    let blocked = matches!(
        pending.safety_report.as_ref().map(|r| r.verdict),
        Some(DraftSafetyVerdict::Blocked)
    );

    let (send, edit) = if pending.opens_on_send() {
        ("\u{203a} [s] send", "[e/Esc] edit again")
    } else {
        ("[s] send", "\u{203a} [e/Esc] edit again")
    };
    let stored = pending.stored_draft.is_some();
    let discard = if stored { "" } else { "   [#] discard draft" };
    let assist = if stored { "" } else { "[r] draft for me   " };
    match pending.mode {
        PendingSendMode::SendOrSave if blocked => {
            let override_action = if pending.override_token.is_some() {
                "   [O] override + send"
            } else {
                ""
            };
            lines.push(format!("{edit}{override_action}{discard}"));
        }
        PendingSendMode::SendOrSave => {
            lines.push(format!(
                "{send}   [a] send at   [n] remind   [d] save draft"
            ));
            lines.push(format!("{assist}{edit}{discard}"));
        }
        PendingSendMode::DraftOnlyNoRecipients => {
            lines.push(format!("[d] save draft   {edit}{discard}"));
        }
        PendingSendMode::Unchanged => {
            lines.push(format!("{edit}{discard}"));
        }
    }
    lines
}

fn push_safety_lines(
    lines: &mut Vec<String>,
    report: &DraftSafetyReport,
    override_token: Option<&str>,
) {
    let verdict_label = match report.verdict {
        DraftSafetyVerdict::Safe => "Safety: SAFE",
        DraftSafetyVerdict::Warn => "Safety: WARN",
        DraftSafetyVerdict::Blocked => "Safety: BLOCKED",
    };
    lines.push(verdict_label.to_string());

    if report.issues.is_empty() {
        lines.push("  no issues".to_string());
        return;
    }

    for issue in &report.issues {
        let sev = match issue.severity {
            DraftSafetySeverity::Info => "info",
            DraftSafetySeverity::Warning => "warn",
            DraftSafetySeverity::Blocker => "BLOCK",
        };
        lines.push(format!("  [{sev}] {}", issue.message));
        for citation in &issue.citations {
            let mid = citation.message_id.as_deref().unwrap_or("?");
            lines.push(format!("       cite msg={mid} field={}", citation.field));
        }
    }

    if matches!(report.verdict, DraftSafetyVerdict::Blocked) {
        if let Some(token) = override_token {
            lines.push(format!("Override token: {token}"));
            lines.push("Press O to override and send (single use).".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::app::{PendingSend, PendingSendMode};
    use mxr_core::{
        DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetyReport, DraftSafetySeverity,
    };
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn pending(mode: PendingSendMode) -> PendingSend {
        PendingSend {
            draft_revision: None,
            account_id: mxr_core::AccountId::new(),
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: "a@example.com".into(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Hello".into(),
                from: "me@example.com".into(),
                in_reply_to: None,
                intent: mxr_core::DraftIntent::New,
                references: vec![],
                thread_id: None,
                attach: vec![],
                signature: None,
                forward_source: None,
            },
            body: "hi".into(),
            draft_path: std::path::PathBuf::from("/tmp/draft.md"),
            intent: mxr_core::DraftIntent::New,
            mode,
            safety_report: None,
            safety_check_failed: None,
            override_token: None,
            suggested_collaborators: vec![],
            invite_reply: None,
            resolved_from: None,
            stored_draft: None,
        }
    }

    #[test]
    fn failed_safety_check_renders_unavailable_warning() {
        let mut p = pending(PendingSendMode::SendOrSave);
        p.safety_check_failed = Some("daemon unreachable".into());
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&p),
                None,
                None,
                &[],
                &crate::theme::Theme::default(),
                &crate::ui::account_mark::AccountMarks::default(),
            );
        });
        assert!(
            rendered.contains("Safety check unavailable: daemon unreachable"),
            "modal must surface the safety-check failure; got:\n{rendered}"
        );
    }

    #[test]
    fn send_or_save_modal_renders_full_action_row() {
        let rendered = render_to_string(120, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 20),
                Some(&pending(PendingSendMode::SendOrSave)),
                None,
                None,
                &[],
                &crate::theme::Theme::default(),
                &crate::ui::account_mark::AccountMarks::default(),
            );
        });

        assert!(rendered.contains("Send this draft?"));
        assert!(rendered.contains("Subject: Hello"));
        assert!(rendered.contains("Voice match: not scored for manual edits"));
        assert!(rendered.contains("Humanizer: scored on AI draft outputs"));
        assert!(rendered.contains("\u{203a} [s] send   [a] send at   [n] remind   [d] save draft"));
        assert!(rendered.contains("[r] draft for me"));
        assert!(rendered.contains("[#] discard draft"));
    }

    #[test]
    fn unfilled_gap_blocks_send_and_offers_edit_without_override() {
        let mut p = pending(PendingSendMode::SendOrSave);
        p.safety_report = Some(DraftSafetyReport::from_issues(vec![DraftSafetyIssue::new(
            DraftSafetyIssueCode::UnfilledGap,
            DraftSafetySeverity::Blocker,
            "Fill in the meeting time before sending.",
        )]));
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&p),
                None,
                None,
                &[],
                &crate::theme::Theme::default(),
                &crate::ui::account_mark::AccountMarks::default(),
            );
        });
        assert!(rendered.contains("Safety: BLOCKED"));
        assert!(rendered.contains("Fill in the meeting time"));
        assert!(rendered.contains("\u{203a} [e/Esc] edit again"));
        assert!(!rendered.contains("[s] send"));
        assert!(!rendered.contains("[O] override + send"));
    }

    fn pending_with_report(
        mode: PendingSendMode,
        report: mxr_core::DraftSafetyReport,
        token: Option<String>,
    ) -> PendingSend {
        let mut p = pending(mode);
        p.safety_report = Some(report);
        p.override_token = token;
        p
    }

    #[test]
    fn safe_verdict_renders_safe_label_and_no_issues() {
        let report = mxr_core::DraftSafetyReport::safe();
        let rendered = render_to_string(120, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 20),
                Some(&pending_with_report(
                    PendingSendMode::SendOrSave,
                    report,
                    None,
                )),
                None,
                None,
                &[],
                &crate::theme::Theme::default(),
                &crate::ui::account_mark::AccountMarks::default(),
            );
        });

        assert!(rendered.contains("Safety: SAFE"));
        assert!(rendered.contains("no issues"));
        assert!(rendered.contains("\u{203a} [s] send"));
        assert!(!rendered.contains("override"));
        insta::assert_snapshot!("safety_modal_safe", rendered);
    }

    #[test]
    fn blocker_verdict_offers_override_token_path() {
        let issues = vec![mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::PiiSecret,
            mxr_core::DraftSafetySeverity::Blocker,
            "secret pattern detected: sk-...abcd",
        )];
        let report = mxr_core::DraftSafetyReport::from_issues(issues);
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&pending_with_report(
                    PendingSendMode::SendOrSave,
                    report,
                    Some("tok-abc-123".into()),
                )),
                None,
                None,
                &[],
                &crate::theme::Theme::default(),
                &crate::ui::account_mark::AccountMarks::default(),
            );
        });

        assert!(rendered.contains("Safety: BLOCKED"));
        assert!(rendered.contains("[BLOCK] secret pattern detected: sk-...abcd"));
        assert!(rendered.contains("Override token: tok-abc-123"));
        assert!(rendered.contains("[O] override + send"));
        assert!(rendered.contains("Press O to override and send"));
        assert!(!rendered.contains("[s] send"));
        insta::assert_snapshot!("safety_modal_blocker", rendered);
    }
}
