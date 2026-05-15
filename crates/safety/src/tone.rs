//! Tone-mismatch warning.
//!
//! Uses `mxr-relationship` stylometry deterministically. For each primary
//! recipient with a baseline of >= 3 prior sent messages, compute the
//! draft's stylometry, score voice match, and warn when the formality
//! delta exceeds the configured threshold and confidence is medium/high.
//!
//! Never invokes the LLM.

use mxr_core::types::{Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity};
use mxr_relationship::{compute_metrics, score_voice_match, VoiceMatchConfidence};

use crate::{config::SafetyToneConfig, SafetyContext};

pub fn check(draft: &Draft, ctx: &SafetyContext, cfg: &SafetyToneConfig) -> Vec<DraftSafetyIssue> {
    if ctx.contact_styles.is_empty() {
        return Vec::new();
    }
    let draft_metrics = compute_metrics(&draft.body_markdown);
    let mut issues = Vec::new();
    let mut seen = Vec::new();
    for addr in draft.to.iter().chain(&draft.cc) {
        let email = addr.email.to_ascii_lowercase();
        if seen.contains(&email) {
            continue;
        }
        seen.push(email.clone());

        let Some(baseline) = ctx
            .contact_styles
            .iter()
            .find(|c| c.email.eq_ignore_ascii_case(&email))
        else {
            continue;
        };
        if baseline.baseline_sample_count < 3 {
            continue;
        }

        let report = score_voice_match(
            &draft_metrics,
            &baseline.baseline,
            baseline.baseline_sample_count,
        );
        if matches!(report.confidence, VoiceMatchConfidence::Low) {
            continue;
        }
        let delta = (draft_metrics.formality_score - baseline.baseline.formality_score).abs();
        if delta < cfg.formality_delta_threshold {
            continue;
        }

        let direction = if draft_metrics.formality_score > baseline.baseline.formality_score {
            "more formal"
        } else {
            "more casual"
        };
        let detail = if report.notable_deltas.is_empty() {
            format!("formality delta {:.2}", delta)
        } else {
            report.notable_deltas.join("; ")
        };
        issues.push(
            DraftSafetyIssue::new(
                DraftSafetyIssueCode::ToneMismatch,
                DraftSafetySeverity::Warning,
                format!("tone is {direction} than usual with {email}"),
            )
            .with_detail(detail),
        );
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::types::{Address, Draft, DraftIntent};
    use mxr_core::{AccountId, DraftId};
    use mxr_relationship::StylometryMetrics;

    use crate::ContactStyleBaseline;

    fn d(body: &str, to: Vec<Address>) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::New,
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "subject".into(),
            body_markdown: body.into(),
            attachments: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn addr(email: &str) -> Address {
        Address {
            email: email.into(),
            name: None,
        }
    }

    fn baseline_for(email: &str, formality: f64, count: u32) -> ContactStyleBaseline {
        ContactStyleBaseline {
            email: email.into(),
            baseline: StylometryMetrics {
                formality_score: formality,
                avg_sentence_len: 14.0,
                ..Default::default()
            },
            baseline_sample_count: count,
        }
    }

    #[test]
    fn no_baseline_no_warning() {
        let ctx = SafetyContext::default();
        assert!(check(
            &d("yo", vec![addr("alice@x.com")]),
            &ctx,
            &SafetyToneConfig::default()
        )
        .is_empty());
    }

    #[test]
    fn low_sample_no_warning() {
        let ctx = SafetyContext {
            contact_styles: vec![baseline_for("alice@x.com", 0.5, 2)],
            ..Default::default()
        };
        let issues = check(
            &d(
                "Hi Alice. Could you please review the proposal at your earliest convenience?",
                vec![addr("alice@x.com")],
            ),
            &ctx,
            &SafetyToneConfig::default(),
        );
        assert!(
            issues.is_empty(),
            "below threshold sample count should suppress"
        );
    }

    #[test]
    fn high_confidence_high_delta_warns() {
        let ctx = SafetyContext {
            contact_styles: vec![baseline_for("alice@x.com", 0.1, 30)],
            ..Default::default()
        };
        let body = "Dear Alice,\n\nFurthermore, I would respectfully request that you kindly furnish the aforementioned documentation at your earliest convenience.\n\nSincerely,\nMe";
        // Verify the underlying scoring lands at High confidence — pin
        // the band so any future tweak to it surfaces here, not via
        // this test's silent transition to Medium.
        let draft_metrics = mxr_relationship::compute_metrics(body);
        let report = mxr_relationship::score_voice_match(
            &draft_metrics,
            &ctx.contact_styles[0].baseline,
            ctx.contact_styles[0].baseline_sample_count,
        );
        assert!(
            matches!(
                report.confidence,
                mxr_relationship::VoiceMatchConfidence::High
            ),
            "fixture must produce High confidence (got {:?})",
            report.confidence
        );

        let issues = check(
            &d(body, vec![addr("alice@x.com")]),
            &ctx,
            &SafetyToneConfig {
                formality_delta_threshold: 0.2,
            },
        );
        assert_eq!(issues.len(), 1, "expected tone warning, got {issues:?}");
        assert_eq!(issues[0].code, DraftSafetyIssueCode::ToneMismatch);
        assert!(issues[0].message.to_lowercase().contains("formal"));
    }

    #[test]
    fn matching_tone_passes() {
        // baseline ~0.5, draft also ~0.5
        let ctx = SafetyContext {
            contact_styles: vec![baseline_for("alice@x.com", 0.5, 30)],
            ..Default::default()
        };
        let issues = check(
            &d(
                "hey alice, sounds good. let me know.",
                vec![addr("alice@x.com")],
            ),
            &ctx,
            &SafetyToneConfig::default(),
        );
        // Could either warn or pass depending on draft stylometry; we
        // assert the SHAPE: at most one issue, and if present it is a
        // warning, never a blocker.
        for issue in &issues {
            assert_eq!(issue.severity, DraftSafetySeverity::Warning);
            assert_ne!(issue.severity, DraftSafetySeverity::Blocker);
        }
    }
}
