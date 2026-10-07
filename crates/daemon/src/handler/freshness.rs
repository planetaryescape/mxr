//! Freshness (`Request::GetFreshness`): when the newest mail came in, how
//! each account's sync is doing, and the last few arrivals with the modes
//! they went to.
//!
//! Nothing new is tracked: the newest arrival is a read of the messages
//! table, sync health comes from the sync runtime status the sync loop
//! already writes (error, failure class, backoff), and the modes come from
//! the same placement `GetModeMembership` uses.

use super::mode_rules::provider_name;
use super::modes::place_threads;
use super::places::scoped_accounts;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::ProviderKind;
use mxr_core::Account;
use mxr_protocol::{
    AccountFreshnessData, ArrivalData, FreshnessData, ResponseData, SyncErrorData,
    SyncErrorKindData, SyncHealthData, ThreadModesData, FRESHNESS_DEFAULT_ARRIVALS,
    FRESHNESS_MAX_ARRIVALS,
};
use mxr_store::{Arrival, SyncRuntimeStatus};
use std::collections::HashMap;

/// A sync that has not worked for this long is stale even with no error:
/// five missed intervals, and never under fifteen minutes, so a slow
/// provider page or a laptop waking from sleep does not cry wolf.
pub(super) fn stale_after_secs(sync_interval_secs: u64) -> u64 {
    sync_interval_secs.saturating_mul(5).max(15 * 60)
}

/// Failure classes that are not failures: a sync still running past a
/// caller's wait, and one cut short by a daemon restart. Neither says the
/// provider is unreachable, and the next pass clears both.
const NOT_FAILURES: &[&str] = &["timeout", "interrupted"];

pub(super) async fn get_freshness(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: Option<u32>,
) -> HandlerResult {
    let limit = limit
        .unwrap_or(FRESHNESS_DEFAULT_ARRIVALS)
        .min(FRESHNESS_MAX_ARRIVALS);
    let now = Utc::now();
    let stale_after = stale_after_secs(state.config_snapshot().general.sync_interval);
    let scope = scoped_accounts(state, account_id).await?;
    let accounts: HashMap<AccountId, Account> = state
        .store
        .list_accounts()
        .await?
        .into_iter()
        .map(|account| (account.id.clone(), account))
        .collect();
    let mut runtime: HashMap<AccountId, SyncRuntimeStatus> = state
        .store
        .list_sync_runtime_statuses()
        .await?
        .into_iter()
        .map(|row| (row.account_id.clone(), row))
        .collect();

    let mut freshness = Vec::new();
    let mut arrivals: Vec<Arrival> = Vec::new();
    for id in &scope {
        let Some(account) = accounts.get(id) else {
            return Err(HandlerError::from(format!("Account not found: {id}")));
        };
        let received = received_mail(state, account, limit).await?;
        let newest = received.first().map(|arrival| arrival.date);
        freshness.push(account_freshness(
            account,
            runtime.remove(id).as_ref(),
            AccountView {
                newest_message_at: newest,
                named_by_provider: accounts.len() == 1,
            },
            now,
            stale_after,
        ));
        arrivals.extend(received);
    }
    arrivals.sort_by_key(|arrival| std::cmp::Reverse(arrival.date));
    arrivals.truncate(usize::try_from(limit).unwrap_or(usize::MAX));

    let worst_account_id = freshness
        .iter()
        .max_by_key(|account| account.health)
        .map(|account| account.account_id.clone());
    Ok(ResponseData::Freshness {
        freshness: FreshnessData {
            generated_at: now,
            newest_message_at: freshness
                .iter()
                .filter_map(|account| account.newest_message_at)
                .max(),
            stale_after_secs: stale_after,
            worst_account_id,
            arrivals: place_arrivals(state, arrivals, now).await?,
            accounts: freshness,
        },
    })
}

/// The account's newest received mail, at least one message so its newest
/// arrival is known when no arrivals were asked for.
async fn received_mail(
    state: &AppState,
    account: &Account,
    limit: u32,
) -> Result<Vec<Arrival>, HandlerError> {
    Ok(state
        .store
        .latest_arrivals(&account.id, limit.max(1))
        .await?)
}

/// What a warning calls the account: "Gmail" or "Outlook" when it is the
/// only account, else its own name, so two Gmail accounts never read the
/// same.
fn account_label(account: &Account, named_by_provider: bool) -> String {
    if !named_by_provider {
        return account.name.clone();
    }
    match account
        .sync_backend
        .as_ref()
        .map(|backend| &backend.provider_kind)
    {
        kind @ Some(
            ProviderKind::Gmail | ProviderKind::OutlookPersonal | ProviderKind::OutlookWork,
        ) => provider_name(kind).to_string(),
        _ => account.name.clone(),
    }
}

/// What the reply knows about an account beyond its sync status.
pub(super) struct AccountView {
    pub newest_message_at: Option<DateTime<Utc>>,
    /// The only account: warnings may call it by its provider ("Gmail").
    pub named_by_provider: bool,
}

pub(super) fn account_freshness(
    account: &Account,
    runtime: Option<&SyncRuntimeStatus>,
    view: AccountView,
    now: DateTime<Utc>,
    stale_after_secs: u64,
) -> AccountFreshnessData {
    let last_sync_error = runtime.and_then(sync_error);
    let last_ok = runtime.and_then(|row| row.last_success_at);
    let in_progress = runtime.is_some_and(|row| row.sync_in_progress);
    let paused_until = runtime
        .and_then(|row| row.backoff_until)
        .filter(|until| *until > now);
    let health = match (&last_sync_error, paused_until) {
        (Some(error), _) if error.kind == SyncErrorKindData::RateLimited => SyncHealthData::Paused,
        (Some(_), _) => SyncHealthData::Failing,
        (None, Some(_)) => SyncHealthData::Paused,
        (None, None) => match last_ok {
            None if in_progress => SyncHealthData::Syncing,
            None => SyncHealthData::Never,
            Some(at)
                if now - at
                    > Duration::seconds(i64::try_from(stale_after_secs).unwrap_or(i64::MAX)) =>
            {
                SyncHealthData::Stale
            }
            Some(_) => SyncHealthData::Ok,
        },
    };
    AccountFreshnessData {
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        label: account_label(account, view.named_by_provider),
        newest_message_at: view.newest_message_at,
        last_sync_ok_at: last_ok,
        last_sync_attempt_at: runtime.and_then(|row| row.last_attempt_at),
        sync_in_progress: in_progress,
        health,
        last_sync_error,
    }
}

/// The stored error as the client sees it, with the retry the rate limit
/// scheduled. Classes that are not failures yield none.
fn sync_error(row: &SyncRuntimeStatus) -> Option<SyncErrorData> {
    let message = row.last_error.clone()?;
    let class = row.failure_class.as_deref();
    if class.is_some_and(|class| NOT_FAILURES.contains(&class)) {
        return None;
    }
    Some(SyncErrorData {
        kind: SyncErrorKindData::from_failure_class(class),
        message,
        retry_at: row.backoff_until,
        consecutive_failures: row.consecutive_failures,
    })
}

/// Each arrival with the modes its conversation is in now, placed the way
/// `GetModeMembership` places it.
async fn place_arrivals(
    state: &AppState,
    arrivals: Vec<Arrival>,
    now: DateTime<Utc>,
) -> Result<Vec<ArrivalData>, HandlerError> {
    let mut by_account: HashMap<AccountId, Vec<ThreadId>> = HashMap::new();
    for arrival in &arrivals {
        let threads = by_account.entry(arrival.account_id.clone()).or_default();
        if !threads.contains(&arrival.thread_id) {
            threads.push(arrival.thread_id.clone());
        }
    }
    let mut placed: HashMap<ThreadId, ThreadModesData> = HashMap::new();
    for (account, threads) in by_account {
        for placement in place_threads(state, &account, &threads, now).await? {
            placed.insert(placement.data.thread_id.clone(), placement.data);
        }
    }
    Ok(arrivals
        .into_iter()
        .map(|arrival| {
            let modes = placed.get(&arrival.thread_id);
            ArrivalData {
                in_inbox: modes.is_some_and(|data| data.in_inbox),
                modes: modes.map(|data| data.modes.clone()).unwrap_or_default(),
                account_id: arrival.account_id,
                message_id: arrival.id,
                thread_id: arrival.thread_id,
                from: arrival.from,
                subject: arrival.subject,
                received_at: arrival.date,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::BackendRef;

    fn gmail() -> Account {
        Account {
            id: AccountId::new(),
            name: "work".into(),
            email: "me@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Gmail,
                config_key: "work".into(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    fn only() -> AccountView {
        AccountView {
            newest_message_at: None,
            named_by_provider: true,
        }
    }

    fn row(account: &Account, now: DateTime<Utc>) -> SyncRuntimeStatus {
        SyncRuntimeStatus {
            account_id: account.id.clone(),
            last_attempt_at: Some(now),
            last_success_at: Some(now - Duration::minutes(2)),
            last_error: None,
            failure_class: None,
            consecutive_failures: 0,
            backoff_until: None,
            sync_in_progress: false,
            current_cursor_summary: None,
            last_synced_count: 0,
            updated_at: now,
        }
    }

    #[test]
    fn a_recent_good_sync_is_ok_and_an_old_one_is_stale() {
        let now = Utc::now();
        let account = gmail();
        let mut status = row(&account, now);
        let fresh = account_freshness(&account, Some(&status), only(), now, 900);
        assert_eq!(fresh.health, SyncHealthData::Ok);
        assert_eq!(fresh.label, "Gmail");

        status.last_success_at = Some(now - Duration::hours(2));
        let stale = account_freshness(&account, Some(&status), only(), now, 900);
        assert_eq!(stale.health, SyncHealthData::Stale);
        assert!(stale.last_sync_error.is_none());
    }

    #[test]
    fn a_rate_limit_pauses_with_its_retry_time() {
        let now = Utc::now();
        let account = gmail();
        let mut status = row(&account, now);
        status.last_error = Some("Provider error: Rate limited, retry after 60s".into());
        status.failure_class = Some("rate_limit".into());
        status.consecutive_failures = 1;
        status.backoff_until = Some(now + Duration::minutes(8));
        let paused = account_freshness(&account, Some(&status), only(), now, 900);
        assert_eq!(paused.health, SyncHealthData::Paused);
        let error = paused.last_sync_error.unwrap();
        assert_eq!(error.kind, SyncErrorKindData::RateLimited);
        assert_eq!(error.retry_at, status.backoff_until);
    }

    #[test]
    fn auth_and_offline_failures_are_failing_with_their_kind() {
        let now = Utc::now();
        let account = gmail();
        for (class, kind) in [
            ("auth", SyncErrorKindData::Auth),
            ("network", SyncErrorKindData::Offline),
        ] {
            let mut status = row(&account, now);
            status.last_error = Some(format!("{class} broke"));
            status.failure_class = Some(class.into());
            status.consecutive_failures = 2;
            let failing = account_freshness(&account, Some(&status), only(), now, 900);
            assert_eq!(failing.health, SyncHealthData::Failing, "{class}");
            assert_eq!(failing.last_sync_error.unwrap().kind, kind, "{class}");
        }
    }

    #[test]
    fn a_restart_or_a_long_running_sync_is_not_a_failure() {
        let now = Utc::now();
        let account = gmail();
        for class in NOT_FAILURES {
            let mut status = row(&account, now);
            status.last_error = Some("sync interrupted by daemon restart".into());
            status.failure_class = Some((*class).into());
            let calm = account_freshness(&account, Some(&status), only(), now, 900);
            assert_eq!(calm.health, SyncHealthData::Ok, "{class}");
        }
    }

    #[test]
    fn a_first_sync_reads_as_syncing_then_never() {
        let now = Utc::now();
        let account = gmail();
        let mut status = row(&account, now);
        status.last_success_at = None;
        status.sync_in_progress = true;
        assert_eq!(
            account_freshness(&account, Some(&status), only(), now, 900).health,
            SyncHealthData::Syncing
        );
        assert_eq!(
            account_freshness(&account, None, only(), now, 900).health,
            SyncHealthData::Never
        );
    }

    #[test]
    fn staleness_waits_for_five_intervals_and_at_least_fifteen_minutes() {
        assert_eq!(stale_after_secs(60), 900);
        assert_eq!(stale_after_secs(600), 3_000);
    }
}
