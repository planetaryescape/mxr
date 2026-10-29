//! Idle exit for daemons that have no real mail to look after.
//!
//! CLI commands auto-start a detached daemon, and nothing stops it again.
//! For a real mailbox that is the point: it keeps syncing. Test harnesses,
//! smoke scripts and demos start daemons on fake accounts and throwaway data
//! directories, and when the harness dies or forgets to stop them they run
//! forever. A daemon with no real account has no background work worth
//! keeping, and the next CLI command starts it again on demand, so it exits
//! once no client has been connected for a while.

use mxr_core::types::{Account, ProviderKind};
use std::time::Duration;

/// Overrides how long a daemon with no real account waits, with no client
/// connected, before it exits. Seconds; `0` turns idle exit off.
pub(crate) const IDLE_EXIT_ENV: &str = "MXR_IDLE_EXIT_SECS";

const DEFAULT_IDLE_EXIT: Duration = Duration::from_secs(30 * 60);

/// How long to wait with no client before exiting, or `None` when turned off.
pub(crate) fn idle_exit_after() -> Option<Duration> {
    parse_idle_exit(std::env::var(IDLE_EXIT_ENV).ok().as_deref())
}

fn parse_idle_exit(value: Option<&str>) -> Option<Duration> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Some(DEFAULT_IDLE_EXIT);
    };
    match value.parse::<u64>() {
        Ok(0) => None,
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => {
            tracing::warn!(
                value,
                "{IDLE_EXIT_ENV} is not a whole number of seconds; using the default"
            );
            Some(DEFAULT_IDLE_EXIT)
        }
    }
}

/// True when an enabled account syncs or sends through a real provider, so
/// the daemon has background work and must stay up.
pub(crate) fn has_real_account(accounts: &[Account]) -> bool {
    accounts
        .iter()
        .filter(|account| account.enabled)
        .any(|account| {
            [&account.sync_backend, &account.send_backend]
                .into_iter()
                .flatten()
                .any(|backend| !matches!(backend.provider_kind, ProviderKind::Fake))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::BackendRef;
    use mxr_core::AccountId;

    fn account(sync: Option<ProviderKind>, send: Option<ProviderKind>, enabled: bool) -> Account {
        let backend = |provider_kind| BackendRef {
            provider_kind,
            config_key: "key".into(),
        };
        Account {
            id: AccountId::new(),
            name: "Test".into(),
            email: "test@example.com".into(),
            sync_backend: sync.map(backend),
            send_backend: send.map(backend),
            enabled,
        }
    }

    #[test]
    fn fake_and_missing_accounts_are_not_real() {
        assert!(!has_real_account(&[]));
        assert!(!has_real_account(&[account(
            Some(ProviderKind::Fake),
            Some(ProviderKind::Fake),
            true
        )]));
    }

    #[test]
    fn a_real_sync_or_send_backend_keeps_the_daemon_up() {
        assert!(has_real_account(&[
            account(Some(ProviderKind::Fake), None, true),
            account(Some(ProviderKind::Gmail), None, true),
        ]));
        assert!(has_real_account(&[account(
            None,
            Some(ProviderKind::Smtp),
            true
        )]));
    }

    #[test]
    fn disabled_real_accounts_do_not_keep_the_daemon_up() {
        assert!(!has_real_account(&[account(
            Some(ProviderKind::Gmail),
            Some(ProviderKind::Gmail),
            false
        )]));
    }

    #[test]
    fn idle_exit_defaults_to_thirty_minutes_and_zero_turns_it_off() {
        assert_eq!(parse_idle_exit(None), Some(DEFAULT_IDLE_EXIT));
        assert_eq!(parse_idle_exit(Some("")), Some(DEFAULT_IDLE_EXIT));
        assert_eq!(parse_idle_exit(Some("0")), None);
        assert_eq!(parse_idle_exit(Some(" 5 ")), Some(Duration::from_secs(5)));
        assert_eq!(parse_idle_exit(Some("soon")), Some(DEFAULT_IDLE_EXIT));
    }
}
