//! Per-boot leases and the remote admission/drain boundary. Registrations live in the store.
use mxr_protocol::{RemotePresence, RemoteSessions, Request};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

const LEASE_SECONDS: i64 = 90;
const MAX_SESSIONS: usize = 1024;

#[derive(Default)]
struct Inner {
    sessions: HashMap<(String, String), RemotePresence>,
    draining: bool,
    in_flight: u64,
    deadline_at: Option<i64>,
}
#[derive(Clone)]
pub(crate) struct Lifecycle {
    boot_id: String,
    inner: Arc<Mutex<Inner>>,
}
impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            boot_id: uuid::Uuid::now_v7().to_string(),
            inner: Default::default(),
        }
    }
}
impl Lifecycle {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub(crate) fn boot_id(&self) -> &str {
        &self.boot_id
    }
    pub(crate) fn touch(
        &self,
        client_id: &str,
        session_id: &str,
        surface: &str,
        now: i64,
    ) -> Result<(), String> {
        if session_id.is_empty()
            || session_id.len() > 80
            || !session_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !matches!(surface, "cli" | "tui" | "mcp" | "web" | "desktop" | "agent")
        {
            return Err("Invalid remote session identity or surface".into());
        }
        let mut inner = self.lock();
        if inner.draining {
            return Err("Host is stopping; reconnect after it returns".into());
        }
        let key = (client_id.to_owned(), session_id.to_owned());
        if inner.sessions.len() >= MAX_SESSIONS && !inner.sessions.contains_key(&key) {
            inner.sessions.retain(|_, session| session.expires_at > now);
        }
        if inner.sessions.len() >= MAX_SESSIONS && !inner.sessions.contains_key(&key) {
            return Err("Too many active remote sessions".into());
        }
        inner.sessions.insert(
            key,
            RemotePresence {
                client_id: client_id.into(),
                session_id: session_id.into(),
                surface: surface.into(),
                last_seen_at: now,
                expires_at: now + LEASE_SECONDS,
                active: true,
            },
        );
        Ok(())
    }
    pub(crate) fn close(&self, client_id: &str, session_id: &str, now: i64) {
        if let Some(session) = self
            .lock()
            .sessions
            .get_mut(&(client_id.into(), session_id.into()))
        {
            session.expires_at = now;
            session.active = false;
        }
    }
    pub(crate) fn close_client(&self, client_id: &str, now: i64) {
        for session in self
            .lock()
            .sessions
            .values_mut()
            .filter(|s| s.client_id == client_id)
        {
            session.active = false;
            session.expires_at = now;
        }
    }
    pub(crate) fn snapshot(&self, now: i64) -> RemoteSessions {
        let inner = self.lock();
        let mut sessions = inner
            .sessions
            .values()
            .cloned()
            .map(|mut s| {
                s.active = s.expires_at > now;
                s
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| (&a.client_id, &a.session_id).cmp(&(&b.client_id, &b.session_id)));
        RemoteSessions {
            boot_id: self.boot_id.clone(),
            draining: inner.draining,
            deadline_at: inner.deadline_at,
            in_flight: inner.in_flight,
            sessions,
        }
    }
    pub(crate) fn drain(&self, now: i64, seconds: u32) -> bool {
        let mut inner = self.lock();
        if inner.draining {
            return false;
        }
        inner.draining = true;
        inner.deadline_at = Some(now + i64::from(seconds.clamp(1, 30)));
        true
    }
    pub(crate) fn resume(&self) {
        let mut inner = self.lock();
        inner.draining = false;
        inner.deadline_at = None;
    }
    pub(crate) fn admit(&self, request: &Request) -> Result<Admission, String> {
        let mut inner = self.lock();
        // Outcome recovery stays available during a planned remote drain.
        if inner.draining
            && !matches!(
                request,
                Request::GetRemoteReady
                    | Request::GetArchiveWorkspace
                    | Request::Ping
                    | Request::GetSendOutcome { .. }
                    | Request::GetArchiveOperation { .. }
                    | Request::GetJob { .. }
            )
        {
            return Err("Host is stopping; preserve drafts and recover accepted outcomes after reconnecting".into());
        }
        inner.in_flight += 1;
        Ok(Admission(self.inner.clone()))
    }
}
pub(crate) struct Admission(Arc<Mutex<Inner>>);
impl Drop for Admission {
    fn drop(&mut self) {
        let mut inner = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.in_flight = inner.in_flight.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leases_expire_without_revoking_registration_and_boots_reset_presence() {
        let lifecycle = Lifecycle::default();
        lifecycle
            .touch("registered", "session", "web", 100)
            .unwrap();
        assert!(lifecycle.snapshot(189).sessions[0].active);
        assert!(!lifecycle.snapshot(190).sessions[0].active);
        let restarted = Lifecycle::default();
        assert!(restarted.snapshot(191).sessions.is_empty());
        assert_ne!(lifecycle.boot_id(), restarted.boot_id());
    }
    #[test]
    fn drain_serializes_admission_and_retains_accepted_work_until_it_finishes() {
        let lifecycle = Lifecycle::default();
        let permit = lifecycle.admit(&Request::Ping).unwrap();
        assert!(lifecycle.drain(100, 5));
        assert!(!lifecycle.drain(102, 5));
        assert!(lifecycle.admit(&Request::GetRemoteReady).is_ok());
        assert!(lifecycle
            .admit(&Request::ListLabels {
                account_id: None,
                all_accounts: false
            })
            .is_err());
        assert!(lifecycle.touch("registered", "other", "cli", 101).is_err());
        assert_eq!(lifecycle.snapshot(101).in_flight, 1);
        drop(permit);
        assert_eq!(lifecycle.snapshot(101).in_flight, 0);
        assert_eq!(lifecycle.snapshot(101).deadline_at, Some(105));
        lifecycle.resume();
        assert!(lifecycle.admit(&Request::GetRemoteReady).is_ok());
    }
    #[test]
    fn closing_one_client_does_not_close_anothers_same_session_name() {
        let lifecycle = Lifecycle::default();
        lifecycle.touch("one", "session", "web", 100).unwrap();
        lifecycle.touch("two", "session", "web", 100).unwrap();
        lifecycle.close("one", "session", 101);
        let snapshot = lifecycle.snapshot(101);
        assert!(!snapshot.sessions[0].active);
        assert!(snapshot.sessions[1].active);
    }
}
