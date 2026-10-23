//! Bounded stored-only classifier reports and truthful policy-history comparison.
use super::assessments::fresh_stored;
use crate::state::AppState;
use mxr_core::{assessment::CLASSIFIER_POLICY_VERSION, AccountId};
use std::collections::HashMap;

/// A report is a stored-only read, including explicit missing answers.
pub(super) async fn report(
    state: &AppState,
    account: Option<&AccountId>,
    limit: u32,
) -> super::HandlerResult {
    let accounts = super::places::scoped_cached_accounts(state, account).await?;
    let limit = limit.clamp(1, 10_000);
    let mut rows = state
        .store
        .assessment_report_rows(&accounts, CLASSIFIER_POLICY_VERSION, limit + 1)
        .await?;
    let truncated = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let mut report = mxr_protocol::ClassifyReportData {
        policy_version: CLASSIFIER_POLICY_VERSION.into(),
        examined: rows.len() as u32,
        missing: 0,
        stale: 0,
        pending_body: 0,
        fallback: 0,
        truncated,
        clusters: Vec::new(),
    };
    let mut fresh = HashMap::new();
    for account in &accounts {
        let scoped = rows
            .iter()
            .flatten()
            .filter(|a| &a.account_id == account)
            .map(|a| (a.message_id.clone(), a.clone()))
            .collect();
        fresh.extend(fresh_stored(state, account, scoped).await?);
    }
    let mut groups = std::collections::BTreeMap::new();
    for row in rows {
        let Some(row) = row else {
            report.missing += 1;
            continue;
        };
        if !fresh.contains_key(&row.message_id) {
            report.stale += 1;
            continue;
        }
        report.pending_body += u32::from(row.pending_body);
        if !row.fallback {
            continue;
        }
        report.fallback += 1;
        let group = groups
            .entry((
                row.home.clone(),
                row.who.clone(),
                row.deciding.rule.clone(),
                row.facts.clone(),
            ))
            .or_insert_with(|| mxr_protocol::ClassifyFallbackClusterData {
                facts: row.facts,
                home: row.home,
                who: row.who,
                rule: row.deciding.rule,
                count: 0,
                examples: Vec::new(),
            });
        group.count += 1;
        if group.examples.len() < 3 {
            group.examples.push(row.message_id);
        }
    }
    report.clusters = groups.into_values().collect();
    report.clusters.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then((&a.home, &a.who, &a.rule).cmp(&(&b.home, &b.who, &b.rule)))
    });
    Ok(mxr_protocol::ResponseData::ClassifyReport { report })
}

pub(super) async fn compare(
    state: &AppState,
    account: Option<&AccountId>,
    policy: &str,
    limit: u32,
) -> super::HandlerResult {
    let accounts = super::places::scoped_cached_accounts(state, account).await?;
    let available_versions = state.store.assessment_versions(&accounts).await?;
    let available = available_versions.iter().any(|v| v == policy);
    let limit = limit.clamp(1, 10_000);
    let mut current = state
        .store
        .assessment_report_rows(&accounts, CLASSIFIER_POLICY_VERSION, limit + 1)
        .await?;
    let truncated = current.len() > limit as usize;
    current.truncate(limit as usize);
    let mut fresh = HashMap::new();
    for account in &accounts {
        let scoped = current
            .iter()
            .flatten()
            .filter(|a| &a.account_id == account)
            .map(|a| (a.message_id.clone(), a.clone()))
            .collect();
        fresh.extend(fresh_stored(state, account, scoped).await?);
    }
    let pending_assessments = current.len().saturating_sub(fresh.len()) as u32;
    let mut previous = HashMap::new();
    if available {
        for account in &accounts {
            let ids: Vec<_> = current
                .iter()
                .flatten()
                .filter(|a| &a.account_id == account)
                .map(|a| a.message_id.clone())
                .collect();
            previous.extend(
                state
                    .store
                    .assessments_for_messages(account, policy, &ids)
                    .await?,
            );
        }
    }
    let compared = fresh.keys().filter(|id| previous.contains_key(*id)).count() as u32;
    let missing_previous_assessments = (fresh.len() as u32).saturating_sub(compared);
    let changes = current
        .into_iter()
        .flatten()
        .filter(|row| fresh.contains_key(&row.message_id))
        .filter_map(|row| {
            let before = previous.get(&row.message_id)?;
            (before.home != row.home).then(|| mxr_protocol::ClassifyHomeChangeData {
                message_id: row.message_id,
                previous_home: before.home.clone(),
                current_home: row.home,
                deciding_reason: row.deciding.reason,
            })
        })
        .collect();
    Ok(mxr_protocol::ResponseData::ClassifyCompare {
        comparison: mxr_protocol::ClassifyComparisonData {
            current_version: CLASSIFIER_POLICY_VERSION.into(),
            requested_version: policy.into(),
            available_versions,
            available,
            pending_assessments,
            compared,
            missing_previous_assessments,
            changes,
            truncated,
        },
    })
}
