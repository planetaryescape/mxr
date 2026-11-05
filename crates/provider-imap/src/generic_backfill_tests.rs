//! Newest-first generic backfill, driven through a range-aware fake server.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "IMAP tests unwrap fixture setup for direct failures"
)]

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mxr_core::id::AccountId;
use mxr_core::provider::MailSyncProvider;
use mxr_core::types::{SyncBatch, SyncCursor};

use crate::config::ImapConfig;
use crate::cursor::ImapCursor;
use crate::error::ImapProviderError;
use crate::session::{ImapSession, ImapSessionFactory};
use crate::types::{
    FetchedMessage, FolderInfo, ImapCapabilities, MailboxInfo, NamespaceInfo, QresyncInfo,
};
use crate::ImapProvider;

struct FakeMailbox {
    uid_validity: u32,
    uid_next: u32,
    uids: BTreeSet<u32>,
    /// UIDs whose FETCH comes back without a body, so parsing fails.
    unparseable: BTreeSet<u32>,
}

#[derive(Default)]
struct World {
    mailboxes: HashMap<String, FakeMailbox>,
    /// (mailbox, uid set) of every UID FETCH, in order.
    fetches: Vec<(String, String)>,
    /// Mailboxes SELECTed, in order.
    selects: Vec<String>,
    /// Every UID SEARCH query, in order.
    searches: Vec<String>,
    /// Fail the Nth (1-based) FETCH once.
    fail_fetch_number: Option<usize>,
}

type Shared = Arc<Mutex<World>>;

fn add_mailbox(world: &Shared, name: &str, uids: impl IntoIterator<Item = u32>) {
    let uids: BTreeSet<u32> = uids.into_iter().collect();
    let uid_next = uids.iter().max().map_or(1, |m| m + 1);
    world.lock().unwrap().mailboxes.insert(
        name.to_string(),
        FakeMailbox {
            uid_validity: 1,
            uid_next,
            uids,
            unparseable: BTreeSet::new(),
        },
    );
}

fn deliver(world: &Shared, mailbox: &str, uid: u32) {
    let mut w = world.lock().unwrap();
    let m = w.mailboxes.get_mut(mailbox).unwrap();
    m.uids.insert(uid);
    m.uid_next = m.uid_next.max(uid + 1);
}

struct FakeSession {
    world: Shared,
    folders: Vec<FolderInfo>,
    selected: Option<String>,
}

fn parse_set(set: &str, max: u32) -> Vec<(u32, u32)> {
    set.split(',')
        .map(|part| match part.split_once(':') {
            Some((a, b)) => {
                let hi = if b == "*" { max } else { b.parse().unwrap() };
                (a.parse().unwrap(), hi)
            }
            None => (part.parse().unwrap(), part.parse().unwrap()),
        })
        .collect()
}

#[async_trait]
impl ImapSession for FakeSession {
    async fn capabilities(&mut self) -> crate::session::Result<ImapCapabilities> {
        Ok(ImapCapabilities::default())
    }
    async fn enable(&mut self, _c: &[&str]) -> crate::session::Result<()> {
        Ok(())
    }
    async fn namespace(&mut self) -> crate::session::Result<Option<NamespaceInfo>> {
        Ok(None)
    }
    async fn select(&mut self, mailbox: &str) -> crate::session::Result<MailboxInfo> {
        let mut w = self.world.lock().unwrap();
        w.selects.push(mailbox.to_string());
        let m = w.mailboxes.get(mailbox).expect("mailbox exists");
        self.selected = Some(mailbox.to_string());
        Ok(MailboxInfo {
            uid_validity: m.uid_validity,
            uid_next: m.uid_next,
            exists: u32::try_from(m.uids.len()).unwrap(),
            highest_modseq: None,
        })
    }
    async fn select_qresync(
        &mut self,
        mailbox: &str,
        _v: u32,
        _m: u64,
        _k: &str,
    ) -> crate::session::Result<QresyncInfo> {
        Ok(QresyncInfo {
            mailbox: self.select(mailbox).await?,
            vanished: vec![],
            changed: vec![],
        })
    }
    async fn uid_fetch(
        &mut self,
        uid_set: &str,
        _query: &str,
    ) -> crate::session::Result<Vec<FetchedMessage>> {
        let mut w = self.world.lock().unwrap();
        let name = self.selected.clone().unwrap();
        w.fetches.push((name.clone(), uid_set.to_string()));
        if w.fail_fetch_number == Some(w.fetches.len()) {
            w.fail_fetch_number = None;
            return Err(ImapProviderError::Connection("injected".into()));
        }
        let m = w.mailboxes.get(&name).unwrap();
        let max = m.uid_next.saturating_sub(1);
        let mut out = Vec::new();
        for (lo, hi) in parse_set(uid_set, max) {
            for uid in m.uids.range(lo..=hi) {
                let mut msg = fake_message(*uid);
                if m.unparseable.contains(uid) {
                    msg.body = None;
                }
                out.push(msg);
            }
        }
        Ok(out)
    }
    async fn uid_search(&mut self, query: &str) -> crate::session::Result<Vec<u32>> {
        let mut w = self.world.lock().unwrap();
        w.searches.push(query.to_string());
        let m = w.mailboxes.get(self.selected.as_ref().unwrap()).unwrap();
        match query.strip_prefix("UID ") {
            Some(set) => {
                let max = m.uid_next.saturating_sub(1);
                Ok(parse_set(set, max)
                    .into_iter()
                    .flat_map(|(lo, hi)| m.uids.range(lo..=hi).copied())
                    .collect())
            }
            None => Ok(m.uids.iter().copied().collect()),
        }
    }
    async fn uid_store(&mut self, _s: &str, _f: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn uid_copy(&mut self, _s: &str, _m: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn uid_move(&mut self, _s: &str, _m: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn uid_append(
        &mut self,
        _m: &str,
        _f: &[&str],
        _b: &[u8],
    ) -> crate::session::Result<Option<u32>> {
        Ok(None)
    }
    async fn uid_expunge(&mut self, _s: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn expunge(&mut self) -> crate::session::Result<()> {
        Ok(())
    }
    async fn list_folders(&mut self) -> crate::session::Result<Vec<FolderInfo>> {
        Ok(self.folders.clone())
    }
    async fn create_mailbox(&mut self, _m: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn rename_mailbox(&mut self, _o: &str, _n: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn delete_mailbox(&mut self, _m: &str) -> crate::session::Result<()> {
        Ok(())
    }
    async fn logout(&mut self) -> crate::session::Result<()> {
        Ok(())
    }
}

fn fake_message(uid: u32) -> FetchedMessage {
    let raw = format!(
        "From: a@example.com\r\nTo: me@test.com\r\nSubject: m{uid}\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nMessage-ID: <m{uid}@test.com>\r\nContent-Type: text/plain\r\n\r\nbody"
    );
    FetchedMessage {
        uid,
        flags: vec![],
        envelope: None,
        body: Some(raw.into_bytes()),
        header: None,
        size: Some(10),
        internal_date: None,
        gmail_labels: vec![],
        gmail_msg_id: None,
        gmail_thread_id: None,
    }
}

struct FakeFactory {
    world: Shared,
    folders: Arc<Mutex<Vec<FolderInfo>>>,
}

#[async_trait]
impl ImapSessionFactory for FakeFactory {
    async fn create_session(&self) -> crate::session::Result<Box<dyn ImapSession>> {
        Ok(Box::new(FakeSession {
            world: self.world.clone(),
            folders: self.folders.lock().unwrap().clone(),
            selected: None,
        }))
    }
}

fn folder(name: &str, special_use: Option<&str>) -> FolderInfo {
    FolderInfo {
        name: name.to_string(),
        special_use: special_use.map(str::to_string),
        ..Default::default()
    }
}

struct Harness {
    world: Shared,
    folders: Arc<Mutex<Vec<FolderInfo>>>,
    provider: ImapProvider,
}

fn harness(folders: Vec<FolderInfo>) -> Harness {
    let world: Shared = Arc::default();
    let folders = Arc::new(Mutex::new(folders));
    let config = ImapConfig::new(
        "imap.test.com".to_string(),
        993,
        "test@test.com".to_string(),
        "test/imap".to_string(),
        true,
        true,
    );
    let provider = ImapProvider::with_session_factory(
        AccountId::new(),
        config,
        Box::new(FakeFactory {
            world: world.clone(),
            folders: folders.clone(),
        }),
    );
    Harness {
        world,
        folders,
        provider,
    }
}

/// Archive is listed before INBOX on purpose: INBOX must still come first.
fn standard_harness(inbox: u32, archive: u32) -> Harness {
    let h = harness(vec![
        folder("Archive", Some("\\Archive")),
        folder("INBOX", Some("\\Inbox")),
    ]);
    add_mailbox(&h.world, "INBOX", 1..=inbox);
    add_mailbox(&h.world, "Archive", 1..=archive);
    h
}

fn ids(batch: &SyncBatch) -> Vec<String> {
    batch
        .upserted
        .iter()
        .map(|m| m.envelope.provider_id.clone())
        .collect()
}

fn uid_of(provider_id: &str) -> u32 {
    provider_id.rsplit(':').next().unwrap().parse().unwrap()
}

async fn run_all_pages(provider: &ImapProvider) -> (Vec<SyncBatch>, SyncCursor) {
    let mut cursor = SyncCursor::empty();
    let mut batches = Vec::new();
    loop {
        let batch = provider.sync_messages(&cursor).await.unwrap();
        cursor = batch.next_cursor.clone();
        let more = batch.has_more;
        batches.push(batch);
        if !more {
            return (batches, cursor);
        }
        assert!(batches.len() < 1000, "backfill did not terminate");
    }
}

fn completed_mailbox(cursor: &SyncCursor, name: &str) -> mxr_core::types::ImapMailboxCursor {
    ImapCursor::decode(cursor)
        .unwrap()
        .unwrap()
        .into_mailboxes()
        .into_iter()
        .find(|m| m.mailbox == name)
        .expect("mailbox in cursor")
}

#[tokio::test]
async fn first_page_is_newest_inbox_then_pages_run_newest_to_oldest() {
    let h = standard_harness(1200, 300);
    let (batches, final_cursor) = run_all_pages(&h.provider).await;

    assert_eq!(batches.len(), 3);
    assert!(batches[0].has_more && batches[1].has_more && !batches[2].has_more);
    let first = ids(&batches[0]);
    assert_eq!(first.len(), 500);
    assert!(first.iter().all(|id| id.starts_with("INBOX:")));
    assert_eq!(first.iter().map(|id| uid_of(id)).min(), Some(701));
    assert_eq!(first.iter().map(|id| uid_of(id)).max(), Some(1200));

    let all: Vec<String> = batches.iter().flat_map(ids).collect();
    assert_eq!(all.len(), 1500);
    let unique: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), 1500, "every message exactly once");
    // Within a folder, each page is entirely below the previous one.
    for name in ["INBOX:", "Archive:"] {
        let pages: Vec<(u32, u32)> = batches
            .iter()
            .filter_map(|b| {
                let uids: Vec<u32> = ids(b)
                    .iter()
                    .filter(|id| id.starts_with(name))
                    .map(|id| uid_of(id))
                    .collect();
                Some((*uids.iter().min()?, *uids.iter().max()?))
            })
            .collect();
        for pair in pages.windows(2) {
            assert!(
                pair[1].1 < pair[0].0,
                "{name} pages must descend: {pages:?}"
            );
        }
    }
    // INBOX is fully listed before any Archive message.
    let last_inbox = all.iter().rposition(|id| id.starts_with("INBOX:")).unwrap();
    let first_archive = all
        .iter()
        .position(|id| id.starts_with("Archive:"))
        .unwrap();
    assert!(last_inbox < first_archive);
    assert_eq!(completed_mailbox(&final_cursor, "INBOX").uid_next, 1201);
    assert_eq!(completed_mailbox(&final_cursor, "Archive").uid_next, 301);
}

#[tokio::test]
async fn mail_arriving_during_backfill_is_left_to_the_first_delta() {
    let h = standard_harness(1200, 10);
    let first = h
        .provider
        .sync_messages(&SyncCursor::empty())
        .await
        .unwrap();
    assert!(first.has_more);

    deliver(&h.world, "INBOX", 1201);

    let mut seen: Vec<String> = ids(&first);
    let mut cursor = first.next_cursor;
    loop {
        let batch = h.provider.sync_messages(&cursor).await.unwrap();
        seen.extend(ids(&batch));
        cursor = batch.next_cursor;
        if !batch.has_more {
            break;
        }
    }
    assert!(!seen.contains(&"INBOX:1201".to_string()));
    assert_eq!(completed_mailbox(&cursor, "INBOX").uid_next, 1201);

    let delta = h.provider.sync_messages(&cursor).await.unwrap();
    let delta_ids = ids(&delta);
    assert_eq!(
        delta_ids.iter().filter(|id| *id == "INBOX:1201").count(),
        1,
        "delta fetches the late arrival exactly once: {delta_ids:?}"
    );
    assert_eq!(
        completed_mailbox(&delta.next_cursor, "INBOX").uid_next,
        1202
    );
}

#[tokio::test]
async fn uidvalidity_change_restarts_only_that_folder() {
    let h = standard_harness(200, 900);
    let first = h
        .provider
        .sync_messages(&SyncCursor::empty())
        .await
        .unwrap();
    // INBOX done (200) and Archive 601..=900 fetched.
    assert_eq!(first.upserted.len(), 500);
    assert!(first.has_more);
    {
        let mut w = h.world.lock().unwrap();
        w.mailboxes.get_mut("Archive").unwrap().uid_validity = 2;
        w.selects.clear();
        w.fetches.clear();
    }

    let (batches, cursor) = run_all_pages_from(&h.provider, first.next_cursor).await;
    let later: Vec<String> = batches.iter().flat_map(ids).collect();
    assert!(
        later.iter().all(|id| id.starts_with("Archive:")),
        "INBOX is not refetched"
    );
    assert_eq!(later.len(), 900, "Archive restarts from its top");
    assert!(later.contains(&"Archive:900".to_string()));
    let w = h.world.lock().unwrap();
    assert!(!w.selects.iter().any(|s| s == "INBOX"));
    drop(w);
    assert_eq!(completed_mailbox(&cursor, "Archive").uid_validity, 2);
    assert_eq!(completed_mailbox(&cursor, "INBOX").uid_validity, 1);
}

async fn run_all_pages_from(
    provider: &ImapProvider,
    start: SyncCursor,
) -> (Vec<SyncBatch>, SyncCursor) {
    let mut cursor = start;
    let mut batches = Vec::new();
    loop {
        let batch = provider.sync_messages(&cursor).await.unwrap();
        cursor = batch.next_cursor.clone();
        let more = batch.has_more;
        batches.push(batch);
        if !more {
            return (batches, cursor);
        }
    }
}

#[tokio::test]
async fn resume_from_stored_cursor_does_not_refetch_completed_pages() {
    let h = standard_harness(1200, 0);
    let first = h
        .provider
        .sync_messages(&SyncCursor::empty())
        .await
        .unwrap();
    let stored = SyncCursor::from_bytes(first.next_cursor.as_bytes().to_vec());

    // A restarted daemon: a brand-new provider over the same server.
    let h2 = harness(h.folders.lock().unwrap().clone());
    h2.world.lock().unwrap().mailboxes = std::mem::take(&mut h.world.lock().unwrap().mailboxes);
    let (batches, _) = run_all_pages_from(&h2.provider, stored).await;
    let resumed: Vec<u32> = batches.iter().flat_map(ids).map(|id| uid_of(&id)).collect();
    assert_eq!(resumed.len(), 700);
    assert!(
        resumed.iter().all(|uid| *uid <= 700),
        "page 1 UIDs 701+ are not refetched"
    );
    let w = h2.world.lock().unwrap();
    assert!(w
        .fetches
        .iter()
        .all(|(_, set)| set.split(',').all(|uid| uid.parse::<u32>().unwrap() <= 700)));
    assert!(w
        .searches
        .iter()
        .all(|q| { q.rsplit(':').next().unwrap().parse::<u32>().unwrap() <= 700 }));
}

#[tokio::test]
async fn parse_failures_floor_uid_next_to_the_lowest_failure_across_pages() {
    let h = standard_harness(700, 0);
    {
        let mut w = h.world.lock().unwrap();
        let inbox = w.mailboxes.get_mut("INBOX").unwrap();
        inbox.unparseable.insert(650); // page 1
        inbox.unparseable.insert(5); // page 2, lower
    }
    let (batches, cursor) = run_all_pages(&h.provider).await;
    let all: Vec<String> = batches.iter().flat_map(ids).collect();
    assert!(!all.contains(&"INBOX:650".to_string()));
    assert!(!all.contains(&"INBOX:5".to_string()));
    assert_eq!(all.len(), 698);
    assert_eq!(completed_mailbox(&cursor, "INBOX").uid_next, 5);

    // The message recovers (server fixes it); the first delta picks it up.
    h.world
        .lock()
        .unwrap()
        .mailboxes
        .get_mut("INBOX")
        .unwrap()
        .unparseable
        .clear();
    let delta = h.provider.sync_messages(&cursor).await.unwrap();
    let delta_ids = ids(&delta);
    assert!(delta_ids.contains(&"INBOX:5".to_string()));
    assert!(delta_ids.contains(&"INBOX:650".to_string()));
}

#[tokio::test]
async fn a_failure_in_the_first_page_survives_into_the_stored_cursor() {
    let h = standard_harness(700, 0);
    h.world
        .lock()
        .unwrap()
        .mailboxes
        .get_mut("INBOX")
        .unwrap()
        .unparseable
        .insert(650);
    let first = h
        .provider
        .sync_messages(&SyncCursor::empty())
        .await
        .unwrap();
    let state = ImapCursor::decode(&first.next_cursor)
        .unwrap()
        .unwrap()
        .into_generic_backfill()
        .expect("mid-backfill cursor is v2");
    assert_eq!(state.pending[0].min_failed_uid, Some(650));
}

#[tokio::test]
async fn a_failed_page_retried_from_the_same_cursor_returns_the_same_page() {
    let h = standard_harness(1200, 0);
    let first = h
        .provider
        .sync_messages(&SyncCursor::empty())
        .await
        .unwrap();
    // Next page's third FETCH fails mid-page (fetches so far: 5 for page 1).
    h.world.lock().unwrap().fail_fetch_number = Some(5 + 3);
    assert!(h.provider.sync_messages(&first.next_cursor).await.is_err());
    let retried = h.provider.sync_messages(&first.next_cursor).await.unwrap();
    let uids: BTreeSet<u32> = ids(&retried).iter().map(|id| uid_of(id)).collect();
    assert_eq!(uids.len(), 500);
    assert_eq!((uids.first(), uids.last()), (Some(&201), Some(&700)));
}

#[tokio::test]
async fn sparse_uid_ranges_and_empty_folders_still_finish() {
    let h = harness(vec![
        folder("INBOX", Some("\\Inbox")),
        folder("Empty", None),
    ]);
    add_mailbox(&h.world, "INBOX", [5, 30_000]);
    add_mailbox(&h.world, "Empty", []);
    let (batches, cursor) = run_all_pages(&h.provider).await;
    let all: BTreeSet<String> = batches.iter().flat_map(ids).collect();
    assert_eq!(
        all,
        BTreeSet::from(["INBOX:5".to_string(), "INBOX:30000".to_string()])
    );
    assert_eq!(batches.len(), 1, "sparse UIDs do not cost extra pages");
    assert_eq!(completed_mailbox(&cursor, "INBOX").uid_next, 30_001);
    assert_eq!(completed_mailbox(&cursor, "Empty").uid_next, 1);
}

#[tokio::test]
async fn a_folder_deleted_mid_backfill_is_dropped_instead_of_wedging() {
    let h = standard_harness(1200, 50);
    let first = h
        .provider
        .sync_messages(&SyncCursor::empty())
        .await
        .unwrap();
    h.folders.lock().unwrap().retain(|f| f.name != "Archive");
    h.world.lock().unwrap().mailboxes.remove("Archive");
    let (batches, cursor) = run_all_pages_from(&h.provider, first.next_cursor).await;
    assert_eq!(batches.iter().map(|b| b.upserted.len()).sum::<usize>(), 700);
    let names: Vec<String> = ImapCursor::decode(&cursor)
        .unwrap()
        .unwrap()
        .into_mailboxes()
        .into_iter()
        .map(|m| m.mailbox)
        .collect();
    assert_eq!(names, vec!["INBOX"]);
}

#[tokio::test]
async fn a_sparse_folder_costs_tens_of_commands_not_thousands() {
    let h = harness(vec![folder("INBOX", Some("\\Inbox"))]);
    // UIDNEXT 200,000 with 50 messages scattered across the whole range.
    let uids: Vec<u32> = (0..50).map(|i| 3 + i * 3_990).collect();
    add_mailbox(&h.world, "INBOX", uids.clone());
    h.world
        .lock()
        .unwrap()
        .mailboxes
        .get_mut("INBOX")
        .unwrap()
        .uid_next = 200_000;

    let (batches, cursor) = run_all_pages(&h.provider).await;

    let got: Vec<u32> = batches.iter().flat_map(ids).map(|id| uid_of(&id)).collect();
    let mut expected = uids;
    expected.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(got, expected, "newest first, each UID once");
    assert_eq!(completed_mailbox(&cursor, "INBOX").uid_next, 200_000);
    let w = h.world.lock().unwrap();
    let commands = w.selects.len() + w.searches.len() + w.fetches.len();
    // SELECT + geometric searches (2k, 8k, 32k, 128k, rest) + one FETCH chunk.
    assert!(commands <= 12, "{commands} commands: {:?}", w.searches);
}
