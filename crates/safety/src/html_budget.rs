//! A body too large or too structurally complex to parse gets no full markup
//! check. That is a warning, not a blocker: the draft goes through, and the
//! outbound gate still refuses the forbidden elements and `javascript:` links a
//! linear scan finds. The notice is shown with the other safety issues in
//! previews, and the send is not held by it.

use mxr_core::types::{DraftContent, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};
use mxr_core::Draft;

pub fn check(draft: &Draft) -> Vec<DraftSafetyIssue> {
    let DraftContent::Html { html, .. } = &draft.content else {
        return Vec::new();
    };
    if mxr_reader::within_budget(html) {
        return Vec::new();
    }
    vec![DraftSafetyIssue::new(
        DraftSafetyIssueCode::HtmlNotFullyChecked,
        DraftSafetySeverity::Warning,
        "HTML too complex to check fully: its size or structure is past the checker's \
         limits, so the markup checks that need a parse were skipped",
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::{Address, DraftIntent};

    fn html_draft(html: String) -> Draft {
        Draft {
            revision: None,
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "sam@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "budget".into(),
            content: DraftContent::Html { html, text: None },
            attachments: Vec::new(),
            managed_attachments: None,
            inline_assets: Vec::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            inline_calendar_reply: None,
        }
    }

    #[test]
    fn an_over_budget_html_body_gets_a_warning_and_no_blocker() {
        let draft = html_draft(format!("<table>{}</table>", "<br>".repeat(200_000)));
        let issues = check(&draft);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, DraftSafetyIssueCode::HtmlNotFullyChecked);
        assert_eq!(issues[0].severity, DraftSafetySeverity::Warning);
    }

    #[test]
    fn an_ordinary_html_body_gets_no_notice() {
        assert!(check(&html_draft("<p>hello</p>".into())).is_empty());
    }
}
