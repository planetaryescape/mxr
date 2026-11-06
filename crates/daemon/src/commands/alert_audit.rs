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
use chrono::{DateTime, Duration, Local, Utc};
use mxr_store::{AuditCandidate, AuditMail};
use mxr_updates::trust::{decide, AlertInput, Decision, History, MIN_HISTORY_MESSAGES};
use mxr_updates::{Fact, WindowKind};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct AuditReport {
    /// Alerts replayed: a sign-in or failed-payment fact on inbound mail.
    pub candidates: usize,
    /// Counts are not a precision grade: the audit has no human truth labels.
    pub precision_evaluated: bool,
    /// How many the rule would have made into a to-do on arrival.
    pub would_create: usize,
    /// How many it would have held, by reason code.
    pub held: BTreeMap<String, usize>,
    /// Candidates by account provider family: `gmail` or `other`.
    pub by_provider: BTreeMap<String, usize>,
    /// Would-create counts by alert kind.
    pub would_create_by_kind: BTreeMap<String, usize>,
}

/// Provider receipts per (account, host), sorted to count earlier mail.
struct Inbound(HashMap<(String, String), Vec<i64>>);

impl Inbound {
    fn new(mail: &[AuditMail]) -> Self {
        let mut map: HashMap<(String, String), Vec<i64>> = HashMap::new();
        for item in mail {
            if let Some(received_at) = item.received_at {
                map.entry((item.account_id.clone(), item.from_host.clone()))
                    .or_default()
                    .push(received_at);
            }
        }
        for dates in map.values_mut() {
            dates.sort_unstable();
        }
        Self(map)
    }

    /// Earlier mail from one host: how many, and the oldest.
    fn history(&self, account: &str, host: &str, before: i64) -> History {
        let Some(dates) = self.0.get(&(account.to_string(), host.to_string())) else {
            return History::default();
        };
        let earlier = dates.partition_point(|date| *date < before);
        History {
            messages: u32::try_from(earlier).unwrap_or(u32::MAX),
            oldest: dates
                .first()
                .filter(|_| earlier > 0)
                .and_then(|date| DateTime::from_timestamp(*date, 0)),
        }
    }

    fn established(&self, account: &str, before: i64) -> Vec<(String, u32)> {
        self.0
            .iter()
            .filter(|((acct, _), _)| acct == account)
            .filter_map(|((_, host), dates)| {
                let earlier = u32::try_from(dates.partition_point(|date| *date < before)).ok()?;
                (earlier >= MIN_HISTORY_MESSAGES).then(|| (host.clone(), earlier))
            })
            .collect()
    }
}

/// Replay the rule over every candidate.
pub fn replay(candidates: &[AuditCandidate], mail: &[AuditMail]) -> AuditReport {
    let inbound = Inbound::new(mail);
    let mut report = AuditReport::default();
    let mut claims = HashSet::new();
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
        let verified = evidence.verified_host.clone();
        let code = evidence.unverified_reason.map(|reason| reason.code());
        let (history, established) = match (&verified, evidence.received_at) {
            (Some(host), Some(at)) => (
                inbound.history(&candidate.account_id, host, at.timestamp()),
                inbound.established(&candidate.account_id, at.timestamp()),
            ),
            _ => (History::default(), Vec::new()),
        };
        let now = evidence
            .received_at
            .map_or_else(Utc::now, |at| at + Duration::minutes(1));
        let input = AlertInput {
            kind,
            enabled: true,
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
            Decision::Create(claim) if claims.insert((candidate.account_id.clone(), claim.key)) => {
                report.would_create += 1;
                *report
                    .would_create_by_kind
                    .entry(kind.as_str().to_string())
                    .or_default() += 1;
            }
            Decision::Create(_) => *report.held.entry("already_claimed".into()).or_default() += 1,
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
    conn.close().await?;
    print!(
        "{}",
        render(&replay(&candidates, &mail), resolve_format(format))?
    );
    Ok(())
}
