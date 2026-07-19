use super::centered_rect;
use crate::app::{PendingSend, PendingSendMode};
use mxr_core::{DraftSafetyReport, DraftSafetySeverity, DraftSafetyVerdict};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    pending: Option<&PendingSend>,
    send_at_input: Option<&str>,
    remind_at_input: Option<&str>,
    theme: &crate::theme::Theme,
) {
    let Some(pending) = pending else {
        return;
    };

    let popup_height_pct = if pending.safety_report.is_some() {
        60
    } else {
        50
    };
    let popup = centered_rect(86, popup_height_pct, area);
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

    let lines = modal_lines(pending, send_at_input, remind_at_input)
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn modal_lines(
    pending: &PendingSend,
    send_at_input: Option<&str>,
    remind_at_input: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![match pending.mode {
        PendingSendMode::SendOrSave => "Send this draft?".to_string(),
        PendingSendMode::DraftOnlyNoRecipients => "No recipients yet. Save as draft?".to_string(),
        PendingSendMode::Unchanged => "Draft unchanged. Discard or keep editing?".to_string(),
    }];

    // Prefer the daemon-resolved effective From (the owned override, or the
    // account primary when `from:` was cleared) so the user sees the exact
    // identity that will send; fall back to the raw `from:` only when the
    // resolve couldn't run (daemon unreachable).
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
        lines.push("Press Ctrl-A to add the top suggestion to Cc.".to_string());
    }

    if let Some(input) = send_at_input {
        lines.push(format!("Send at: {input}"));
        lines.push("Enter to schedule. Esc to cancel prompt.".to_string());
    }
    if let Some(input) = remind_at_input {
        lines.push(format!("Remind at: {input}"));
        lines.push("Enter to send + set reminder. Esc to cancel prompt.".to_string());
    }
    lines.push(String::new());

    let blocked = matches!(
        pending.safety_report.as_ref().map(|r| r.verdict),
        Some(DraftSafetyVerdict::Blocked)
    );

    match pending.mode {
        PendingSendMode::SendOrSave if blocked => {
            lines.push("[e] edit again   [^O] override + send   [Esc] discard".to_string());
        }
        PendingSendMode::SendOrSave => {
            lines.push("[s] send   [a] send at   [n] remind   [d] save draft".to_string());
            lines.push("[r] refine   [e] edit again   [Esc] discard".to_string());
        }
        PendingSendMode::DraftOnlyNoRecipients => {
            lines.push("[d] save draft   [e] edit again   [Esc] discard".to_string());
        }
        PendingSendMode::Unchanged => {
            lines.push("[e] edit again   [Esc] discard".to_string());
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
            lines.push("Press Ctrl-O to override and send (single use).".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::app::{PendingSend, PendingSendMode};
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn pending(mode: PendingSendMode) -> PendingSend {
        PendingSend {
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
        }
    }

    /// Phase 4: a failed pre-send safety check is no longer silent —
    /// the modal says why no safety block is shown, so the user can
    /// tell "checked clean" apart from "check never ran".
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
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            rendered.contains("Safety check unavailable: daemon unreachable"),
            "modal must surface the safety-check failure; got:\n{rendered}"
        );
    }

    /// And when a report did come back, the unavailable line must not
    /// appear even if a stale failure string is set.
    #[test]
    fn safety_report_takes_precedence_over_failure_line() {
        let mut p = pending(PendingSendMode::SendOrSave);
        p.safety_report = Some(mxr_core::DraftSafetyReport::safe());
        p.safety_check_failed = Some("stale".into());
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&p),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });
        assert!(!rendered.contains("Safety check unavailable"));
        assert!(rendered.contains("Safety: SAFE"));
    }

    /// The modal shows the daemon-resolved effective From — including the
    /// primary-fallback identity when the user cleared the `from:` field — so
    /// the sender always sees the exact address the message will send from.
    #[test]
    fn modal_shows_resolved_effective_from_when_frontmatter_cleared() {
        let mut p = pending(PendingSendMode::SendOrSave);
        p.fm.from = String::new(); // user cleared `from:`
        p.resolved_from = Some(mxr_core::types::Address {
            name: Some("Demo".into()),
            email: "primary@example.com".into(),
        });
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&p),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            rendered.contains("From: Demo <primary@example.com>"),
            "modal must show the resolved effective From; got:\n{rendered}"
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
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Send this draft?"));
        assert!(rendered.contains("Subject: Hello"));
        assert!(rendered.contains("Voice match: not scored for manual edits"));
        assert!(rendered.contains("Humanizer: scored on AI draft outputs"));
        assert!(rendered.contains("[s] send   [a] send at   [n] remind   [d] save draft"));
        assert!(rendered.contains("[r] refine"));
        assert!(rendered.contains("[Esc] discard"));
    }

    #[test]
    fn send_at_prompt_renders_inline_input() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::SendOrSave)),
                Some("in 2h"),
                None,
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Send at: in 2h"));
        assert!(rendered.contains("Enter to schedule"));
    }

    #[test]
    fn remind_prompt_renders_inline_input() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::SendOrSave)),
                None,
                Some("friday 10am"),
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Remind at: friday 10am"));
        assert!(rendered.contains("Enter to send + set reminder"));
    }

    #[test]
    fn missing_recipient_modal_renders_draft_only_actions() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::DraftOnlyNoRecipients)),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("No recipients yet. Save as draft?"));
        assert!(rendered.contains("[d] save draft   [e] edit again   [Esc] discard"));
        assert!(!rendered.contains("[s] send"));
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
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Safety: SAFE"));
        assert!(rendered.contains("no issues"));
        assert!(rendered.contains("[s] send"));
        assert!(!rendered.contains("override"));
        // Snapshot lock: any visible drift forces an insta review.
        // The contains asserts above catch specific removals; the
        // snapshot catches additions/relayouts the asserts miss.
        insta::assert_snapshot!("safety_modal_safe", rendered);
    }

    #[test]
    fn warn_verdict_renders_each_warning_message() {
        let issues = vec![mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::AnswerCoverage,
            mxr_core::DraftSafetySeverity::Warning,
            "draft does not address: who owns rollout?",
        )
        .with_citations(vec![mxr_core::CitationRef {
            message_id: Some("msg-99".into()),
            thread_id: Some("th-1".into()),
            field: "body".into(),
            quote: "who owns rollout?".into(),
        }])];
        let report = mxr_core::DraftSafetyReport::from_issues(issues);
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&pending_with_report(
                    PendingSendMode::SendOrSave,
                    report,
                    None,
                )),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Safety: WARN"));
        assert!(rendered.contains("[warn] draft does not address: who owns rollout?"));
        assert!(rendered.contains("cite msg=msg-99 field=body"));
        // Warnings still allow send.
        assert!(rendered.contains("[s] send"));
        assert!(!rendered.contains("override"));
        insta::assert_snapshot!("safety_modal_warn", rendered);
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
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Safety: BLOCKED"));
        assert!(rendered.contains("[BLOCK] secret pattern detected: sk-...abcd"));
        // Override token must be visible to user (copy-pasteable).
        assert!(rendered.contains("Override token: tok-abc-123"));
        assert!(rendered.contains("[^O] override + send"));
        // Blocked verdict suppresses the normal [s] send shortcut and [r] refine.
        assert!(!rendered.contains("[s] send"));
        assert!(!rendered.contains("[r] refine"));
        insta::assert_snapshot!("safety_modal_blocker", rendered);
    }

    #[test]
    fn blocker_without_token_omits_override_affordance() {
        let issues = vec![mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::NoRecipients,
            mxr_core::DraftSafetySeverity::Blocker,
            "no recipients",
        )];
        let report = mxr_core::DraftSafetyReport::from_issues(issues);
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&pending_with_report(
                    PendingSendMode::SendOrSave,
                    report,
                    None,
                )),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Safety: BLOCKED"));
        assert!(!rendered.contains("Override token:"));
        insta::assert_snapshot!("safety_modal_blocker_no_token", rendered);
    }

    /// `safety_modal_after_edit_rerun` (per doc Slice 1.5 spec):
    /// snapshot of the modal after the user has edited the draft and
    /// re-run the safety check, with a fresh report attached. The
    /// rendered shape must match `safety_modal_safe` exactly when
    /// the rerun cleared the verdict to Safe — i.e. there's no
    /// stale "BLOCKED" residue.
    #[test]
    fn safety_modal_after_edit_rerun_clears_to_safe() {
        // Step 1: snapshot a Blocked modal.
        let blocker_issues = vec![mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::PiiSecret,
            mxr_core::DraftSafetySeverity::Blocker,
            "secret pattern detected: sk-...abcd",
        )];
        let mut p = pending_with_report(
            PendingSendMode::SendOrSave,
            mxr_core::DraftSafetyReport::from_issues(blocker_issues),
            Some("tok-abc".into()),
        );
        // Step 2: simulate the user re-editing + re-checking → Safe.
        p.safety_report = Some(mxr_core::DraftSafetyReport::safe());
        p.override_token = None;
        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                Some(&p),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Safety: SAFE"));
        assert!(!rendered.contains("BLOCKED"));
        assert!(!rendered.contains("Override token:"));
        assert!(rendered.contains("[s] send"));
        insta::assert_snapshot!("safety_modal_after_edit_rerun", rendered);
    }

    #[test]
    fn unchanged_modal_renders_without_send_or_save_actions() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::Unchanged)),
                None,
                None,
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Draft unchanged. Discard or keep editing?"));
        assert!(rendered.contains("[e] edit again   [Esc] discard"));
        assert!(!rendered.contains("[s] send"));
        assert!(!rendered.contains("[d] save draft"));
    }
}
