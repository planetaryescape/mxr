//! `mxr updates alert-audit`: replay the automatic alert to-do rule over the
//! local store (blueprint 22, D193).
//!
//! Opens the database read-only without the daemon, so nothing is written and
//! no mail leaves the machine. It prints counts by outcome and reason only:
//! no subjects, senders, hosts or bodies. It uses recorded arrival
//! provenance and current stored
//! eligibility. It cannot grade semantic precision without human labels.

use crate::cli::OutputFormat;
use crate::output::resolve_format;
use chrono::{DateTime, Local, Utc};
use mxr_store::{AuditCandidate, AuditMail};
use mxr_updates::trust::{decide, AlertInput, Decision, History, MIN_HISTORY_MESSAGES};
use mxr_updates::{Fact, WindowKind};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct AuditReport {
    /// Alerts replayed: a sign-in or failed-payment fact on inbound mail.
    pub candidates: usize,
    pub auto_todo_enabled: bool,
    /// Counts are not a precision grade: the audit has no human truth labels.
    pub precision_evaluated: bool,
    /// How many currently satisfy the stored eligibility and freshness gates.
    pub would_create: usize,
    /// How many it would have held, by reason code.
    pub held: BTreeMap<String, usize>,
    /// Candidates by account provider family: `gmail` or `other`.
    pub by_provider: BTreeMap<String, usize>,
    /// Would-create counts by alert kind.
    pub would_create_by_kind: BTreeMap<String, usize>,
}

type InboundKey = (String, String);
type InboundEntry = (i64, String, i64);
type InboundMap = HashMap<InboundKey, Vec<InboundEntry>>;

/// Provider receipts per (account, host), sorted to count earlier mail.
struct Inbound(InboundMap);

impl Inbound {
    fn new(mail: &[AuditMail]) -> Self {
        let mut map: InboundMap = HashMap::new();
        for item in mail {
            if let Some(received_at) = item.received_at {
                map.entry((item.account_id.clone(), item.from_host.clone()))
                    .or_default()
                    .push((
                        received_at,
                        item.message_fingerprint.clone(),
                        item.observed_at,
                    ));
            }
        }
        for dates in map.values_mut() {
            dates.sort_unstable();
        }
        Self(map)
    }

    /// Earlier mail from one host: how many, and the oldest.
    fn history(&self, account: &str, host: &str, before: i64, candidate: Option<&str>) -> History {
        let Some(dates) = self.0.get(&(account.to_string(), host.to_string())) else {
            return History::default();
        };
        let prior: std::collections::HashMap<&str, i64> = dates
            .iter()
            .filter(|(date, fingerprint, observed)| {
                *date < before && *observed < before && Some(fingerprint.as_str()) != candidate
            })
            .fold(
                std::collections::HashMap::new(),
                |mut out, (date, fingerprint, _)| {
                    out.entry(fingerprint.as_str())
                        .and_modify(|d| *d = (*d).min(*date))
                        .or_insert(*date);
                    out
                },
            );
        let earlier = prior.len();
        History {
            messages: u32::try_from(earlier).unwrap_or(u32::MAX),
            oldest: prior
                .values()
                .min()
                .and_then(|date| DateTime::from_timestamp(*date, 0)),
        }
    }

    fn established(&self, account: &str, before: i64) -> Vec<(String, u32)> {
        self.0
            .iter()
            .filter(|((acct, _), _)| acct == account)
            .filter_map(|((_, host), dates)| {
                let earlier = u32::try_from(
                    dates
                        .iter()
                        .filter(|(date, _, observed)| *date < before && *observed < before)
                        .map(|(_, fingerprint, _)| fingerprint)
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                )
                .ok()?;
                (earlier >= MIN_HISTORY_MESSAGES).then(|| (host.clone(), earlier))
            })
            .collect()
    }
}

/// Replay the rule over every candidate.
fn replay(
    candidates: &[AuditCandidate],
    mail: &[AuditMail],
    existing: &[(String, String, String, String)],
    enabled: bool,
    now: DateTime<Utc>,
) -> AuditReport {
    let inbound = Inbound::new(mail);
    let mut report = AuditReport {
        auto_todo_enabled: enabled,
        ..Default::default()
    };
    let mut claims: HashSet<_> = existing
        .iter()
        .map(|(account, key, _, _)| (account.clone(), key.clone()))
        .collect();
    let mut originals: HashSet<_> = existing
        .iter()
        .map(|(account, _, fingerprint, kind)| (account.clone(), fingerprint.clone(), kind.clone()))
        .collect();
    for candidate in candidates {
        let Ok(fact) = serde_json::from_str::<Fact>(&candidate.fact_json) else {
            continue;
        };
        let Some(kind) = fact.needs_you else {
            continue;
        };
        report.candidates += 1;
        let gmail = candidate
            .sync_provider
            .as_deref()
            .is_some_and(|provider| provider.to_ascii_lowercase().contains("gmail"));
        *report
            .by_provider
            .entry(if gmail { "gmail" } else { "other" }.to_string())
            .or_default() += 1;
        let evidence = &candidate.arrival;
        let verified = evidence
            .message_fingerprint
            .as_ref()
            .and_then(|_| evidence.verified_host.clone());
        let code = evidence
            .unverified_reason
            .map(mxr_core::UnverifiedReason::code);
        let (history, established) = match (&verified, evidence.received_at) {
            (Some(host), Some(at)) => (
                inbound.history(
                    &candidate.account_id,
                    host,
                    at.timestamp(),
                    evidence.message_fingerprint.as_deref(),
                ),
                inbound.established(&candidate.account_id, at.timestamp()),
            ),
            _ => (History::default(), Vec::new()),
        };
        let input = AlertInput {
            kind,
            enabled,
            verified_host: verified.as_deref(),
            unverified_code: code,
            received_at: evidence.received_at,
            now,
            in_inbox: candidate.in_inbox,
            is_live: evidence.is_live,
            unread: candidate.unread,
            list_mail: candidate.has_list_id,
            window_until: fact
                .window
                .as_ref()
                .filter(|window| window.kind == WindowKind::SignIn)
                .map(|window| window.until),
            history,
            established: &established,
        };
        match decide(&input, &Local) {
            Decision::Create(claim) => {
                let original = (
                    candidate.account_id.clone(),
                    evidence.message_fingerprint.clone().unwrap_or_default(),
                    kind.as_str().to_string(),
                );
                if !claims.insert((candidate.account_id.clone(), claim.key))
                    || !originals.insert(original)
                {
                    *report.held.entry("already_claimed".into()).or_default() += 1;
                    continue;
                }
                report.would_create += 1;
                *report
                    .would_create_by_kind
                    .entry(kind.as_str().to_string())
                    .or_default() += 1;
            }
            Decision::Hold(held) => *report.held.entry(held.code()).or_default() += 1,
        }
    }
    report
}

pub fn render(report: &AuditReport, format: OutputFormat) -> anyhow::Result<String> {
    if matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
        return Ok(format!("{}\n", serde_json::to_string(report)?));
    }
    let mut out = format!(
        "{} stored alerts assessed. {} eligible claims under recorded provenance.\n",
        report.candidates, report.would_create
    );
    for (reason, count) in &report.held {
        out.push_str(&format!("  held: {reason}: {count}\n"));
    }
    out.push_str("Counts only: nothing was written and no sender or subject is shown. Precision needs human labels; these counts do not measure false tasks.\n");
    Ok(out)
}

pub async fn run(format: Option<OutputFormat>) -> anyhow::Result<()> {
    use sqlx::Connection;
    let db = mxr_config::data_dir().join("mxr.db");
    anyhow::ensure!(db.exists(), "No mxr database at {}.", db.display());
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db)
        .read_only(true)
        .create_if_missing(false);
    let mut conn = sqlx::SqliteConnection::connect_with(&options).await?;
    let candidates = mxr_store::alert_audit_candidates(&mut conn).await?;
    let mail = mxr_store::alert_audit_inbound(&mut conn).await?;
    let existing = mxr_store::alert_audit_claims(&mut conn).await?;
    let enabled = mxr_config::load_config()?.updates.auto_todo_alerts;
    conn.close().await?;
    print!(
        "{}",
        render(
            &replay(&candidates, &mail, &existing, enabled, Utc::now()),
            resolve_format(format)
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_respect_real_clock_configuration_and_enduring_original_claims() {
        let now = Utc::now();
        let host = "stripe.com";
        let fact = mxr_updates::derive(
            &mxr_updates::FactInput {
                subject: "Your payment failed",
                body: None,
                snippet: "",
                from_email: "notifications@stripe.com",
                from_name: None,
                list_id: None,
                date: now,
            },
            &Local,
        );
        let mut candidates = vec![AuditCandidate {
            account_id: "account-a".into(),
            sync_provider: Some("gmail".into()),
            has_list_id: false,
            fact_json: serde_json::to_string(&fact).expect("fact"),
            in_inbox: true,
            unread: true,
            arrival: mxr_store::StoredArrival {
                received_at: Some(now - chrono::Duration::minutes(1)),
                is_live: true,
                verified_host: Some(host.into()),
                unverified_reason: None,
                message_fingerprint: Some("candidate-original".into()),
            },
        }];
        let mail: Vec<_> = [60, 40, 20]
            .into_iter()
            .map(|days| AuditMail {
                account_id: "account-a".into(),
                from_host: host.into(),
                received_at: Some((now - chrono::Duration::days(days)).timestamp()),
                observed_at: (now - chrono::Duration::days(days)).timestamp(),
                message_fingerprint: format!("prior-{days}"),
            })
            .collect();
        assert_eq!(replay(&candidates, &mail, &[], true, now).would_create, 1);
        let old_claim = vec![(
            "account-a".into(),
            "alert|stripe.com|payment_failed|older-day".into(),
            "candidate-original".into(),
            "payment_failed".into(),
        )];
        let claimed = replay(&candidates, &mail, &old_claim, true, now);
        assert_eq!(claimed.would_create, 0);
        assert_eq!(claimed.held.get("already_claimed"), Some(&1));
        assert_eq!(
            replay(&candidates, &mail, &[], false, now)
                .held
                .get("switched_off"),
            Some(&1)
        );
        candidates[0].arrival.received_at = Some(now - chrono::Duration::hours(25));
        assert_eq!(
            replay(&candidates, &mail, &[], true, now)
                .held
                .get("too_old"),
            Some(&1)
        );
        let rendered = render(&claimed, OutputFormat::Json).expect("counts JSON");
        assert!(!rendered.contains(host));
        assert!(!rendered.contains("candidate-original"));
        assert!(!claimed.precision_evaluated);
    }
}
