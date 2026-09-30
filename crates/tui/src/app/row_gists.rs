//! List-row gists in the TUI: what each conversation is about and what it
//! asks of you, shown in place of the snippet (mail list) and under the
//! row (desk). The same daemon request as the web (`GetThreadGists`): the
//! rows on screen are asked for, cached gists come back at once, and the
//! rest arrive as `ThreadGistReady` events.

use super::{App, MailboxView};
use mxr_core::id::ThreadId;
use mxr_core::types::LabelKind;
use mxr_protocol::{
    GistModelData, ThreadGistBatchData, ThreadGistData, ThreadGistStatusData,
    THREAD_GISTS_MAX_BATCH,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Rows past the cursor whose gists are asked for.
const LOOKAHEAD: usize = 40;
/// Rows before the cursor, for scrolling back up.
const LOOKBEHIND: usize = 10;
/// A row asked about isn't asked about again until this passes.
const REQUEST_TTL: Duration = Duration::from_secs(60);
/// At most one request this often, so holding `j` sends a handful.
const MIN_INTERVAL: Duration = Duration::from_millis(300);
/// With no model, stop asking for this long.
const NO_MODEL_RECHECK: Duration = Duration::from_secs(5 * 60);

/// One row's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowGist {
    pub about: String,
    /// The model's summary of the ask, when there is one.
    pub ask: Option<String>,
    /// The list line, built once: rows are drawn every frame.
    line: String,
}

impl RowGist {
    fn from_data(data: &ThreadGistData) -> Option<Self> {
        if data.status != ThreadGistStatusData::Ready {
            return None;
        }
        let about = data.gist.as_deref()?.trim();
        if about.is_empty() {
            return None;
        }
        let ask = data
            .ask
            .as_ref()
            .map(|ask| ask.summary.trim().to_string())
            .filter(|ask| !ask.is_empty());
        Some(Self::new(about.to_string(), ask))
    }

    pub fn new(about: String, ask: Option<String>) -> Self {
        let line = match &ask {
            Some(ask) => format!("asks: {ask} \u{b7} {about}"),
            None => about.clone(),
        };
        Self { about, ask, line }
    }

    /// "asks: confirm the owner · Canary stays at 5%", ask first.
    pub fn line(&self) -> &str {
        &self.line
    }
}

pub type RowGists = HashMap<ThreadId, RowGist>;

/// Most rows remembered, shown or asked about; the oldest go first.
const STATE_CAP: usize = 500;

#[derive(Debug, Default)]
pub struct RowGistState {
    pub gists: RowGists,
    /// What the daemon last said about its model. The desk reserves a line
    /// per row only when it is `Available`.
    pub model: Option<GistModelData>,
    /// When each gist was stored, for dropping the oldest past the cap.
    stored: HashMap<ThreadId, u64>,
    stored_seq: u64,
    requested: HashMap<ThreadId, Instant>,
    /// Bumped whenever shown gists may no longer hold: a request answered
    /// from an older epoch is dropped, so it can't put back what an
    /// invalidation took away.
    epoch: u64,
    off_until: Option<Instant>,
    /// When rows were last gathered, sent or not: an idle list isn't
    /// rebuilt on every pass of the event loop.
    last_look: Option<Instant>,
}

impl RowGistState {
    /// Whether a request may go out now: not too soon after the last look,
    /// and not while the daemon has no model. Check before gathering rows.
    pub fn due(&self, now: Instant) -> bool {
        !self.off_until.is_some_and(|until| now < until)
            && !self
                .last_look
                .is_some_and(|at| now.duration_since(at) < MIN_INTERVAL)
    }

    /// Reserve a gist line under desk rows: the daemon has a usable model.
    pub fn lines_reserved(&self) -> bool {
        self.model == Some(GistModelData::Available)
    }

    /// The epoch a request sent now belongs to.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The ids to ask about now, in order, or `None` when there is nothing
    /// new or it is too soon. Marks them asked.
    pub fn take_request(
        &mut self,
        candidates: impl IntoIterator<Item = ThreadId>,
        now: Instant,
    ) -> Option<Vec<ThreadId>> {
        if !self.due(now) {
            return None;
        }
        self.last_look = Some(now);
        let mut seen = HashSet::new();
        let mut wanted = Vec::new();
        for id in candidates {
            if wanted.len() == THREAD_GISTS_MAX_BATCH {
                break;
            }
            let asked = self
                .requested
                .get(&id)
                .is_some_and(|at| now.duration_since(*at) < REQUEST_TTL);
            if self.gists.contains_key(&id) || asked || !seen.insert(id.clone()) {
                continue;
            }
            wanted.push(id);
        }
        if wanted.is_empty() {
            return None;
        }
        for id in &wanted {
            self.requested.insert(id.clone(), now);
        }
        while self.requested.len() > STATE_CAP {
            let Some(oldest) = self
                .requested
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.requested.remove(&oldest);
        }
        Some(wanted)
    }

    /// The answer to a request sent in `epoch` for `ids`. One from before
    /// an invalidation is dropped, and its rows may be asked about again.
    pub fn apply_batch(
        &mut self,
        ids: &[ThreadId],
        epoch: u64,
        batch: &ThreadGistBatchData,
        now: Instant,
    ) {
        if epoch != self.epoch {
            self.request_failed(ids);
            return;
        }
        self.model = Some(batch.model);
        if batch.model == GistModelData::Disabled {
            self.off_until = Some(now + NO_MODEL_RECHECK);
            return;
        }
        for gist in &batch.gists {
            self.put(gist);
        }
    }

    /// A failed request: let the rows be asked about again.
    pub fn request_failed(&mut self, ids: &[ThreadId]) {
        for id in ids {
            self.requested.remove(id);
        }
    }

    /// A gist from the daemon: an answer or a `ThreadGistReady` event. The
    /// daemon only hands out gists for a conversation as it is now.
    pub fn put(&mut self, data: &ThreadGistData) {
        let Some(gist) = RowGist::from_data(data) else {
            return;
        };
        self.stored_seq += 1;
        self.stored.insert(data.thread_id.clone(), self.stored_seq);
        self.gists.insert(data.thread_id.clone(), gist);
        while self.gists.len() > STATE_CAP {
            let Some(oldest) = self
                .stored
                .iter()
                .min_by_key(|(_, seq)| **seq)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.stored.remove(&oldest);
            self.gists.remove(&oldest);
        }
    }

    /// New messages changed these conversations: drop their gists and let
    /// their rows ask again. Answers already on their way are dropped.
    pub fn changed<'a>(&mut self, ids: impl IntoIterator<Item = &'a ThreadId>) {
        self.epoch += 1;
        for id in ids {
            self.gists.remove(id);
            self.stored.remove(id);
            self.requested.remove(id);
        }
    }

    /// Nothing shown can be trusted (missed or sampled events, a
    /// reconnect): drop it all; the rows on screen ask again.
    pub fn invalidate_all(&mut self) {
        self.epoch += 1;
        self.gists.clear();
        self.stored.clear();
        self.requested.clear();
    }
}

impl App {
    /// Thread ids of the rows around the cursor, cursor first, when the
    /// current screen is a triage list (the desk, the inbox, a label).
    pub(crate) fn row_gist_candidates(&self) -> Vec<ThreadId> {
        let ids: Vec<ThreadId> = match self.mailbox.mailbox_view {
            MailboxView::Desk => self
                .mailbox
                .desk_page
                .lanes
                .iter()
                .flat_map(|(_, lane)| lane.rows.iter().map(|row| row.thread_id.clone()))
                .collect(),
            MailboxView::Messages if self.triage_label_active() => self
                .mail_list_rows()
                .into_iter()
                .map(|row| row.thread_id)
                .collect(),
            _ => return Vec::new(),
        };
        around(&ids, self.mailbox.selected_index)
    }

    fn triage_label_active(&self) -> bool {
        let Some(active) = self.mailbox.active_label.as_ref() else {
            return false;
        };
        self.mailbox
            .labels
            .iter()
            .find(|label| &label.id == active)
            .is_some_and(|label| label.provider_id == "INBOX" || label.kind == LabelKind::User)
    }
}

/// From the cursor down, then just above it.
fn around(ids: &[ThreadId], cursor: usize) -> Vec<ThreadId> {
    let cursor = cursor.min(ids.len());
    let below = ids.iter().skip(cursor).take(LOOKAHEAD);
    let above = ids[cursor.saturating_sub(LOOKBEHIND)..cursor].iter().rev();
    below.chain(above).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::ThreadAskData;

    fn ready(id: &ThreadId, ask: Option<&str>) -> ThreadGistData {
        ThreadGistData {
            thread_id: id.clone(),
            status: ThreadGistStatusData::Ready,
            gist: Some("Canary stays at 5%.".into()),
            ask: ask.map(|summary| ThreadAskData {
                summary: summary.into(),
                quote: None,
            }),
            provenance: None,
            reason: None,
            generated_at: None,
            from_cache: true,
            newest_message_id: None,
        }
    }

    #[test]
    fn asks_once_per_row_and_not_too_often() {
        let (a, b) = (ThreadId::new(), ThreadId::new());
        let mut state = RowGistState::default();
        let start = Instant::now();
        assert_eq!(
            state.take_request([a.clone(), b.clone(), a.clone()], start),
            Some(vec![a.clone(), b.clone()])
        );
        let soon = start + Duration::from_millis(100);
        assert_eq!(
            state.take_request([ThreadId::new()], soon),
            None,
            "rate limited"
        );
        let later = start + Duration::from_secs(1);
        assert_eq!(
            state.take_request([a.clone(), b.clone()], later),
            None,
            "asked already"
        );
        state.put(&ready(&a, Some("confirm the owner")));
        let after_ttl = start + REQUEST_TTL + Duration::from_secs(1);
        assert_eq!(
            state.take_request([a.clone(), b.clone()], after_ttl),
            Some(vec![b]),
            "a shown row isn't asked about again"
        );
        assert_eq!(
            state.gists[&a].line(),
            "asks: confirm the owner \u{b7} Canary stays at 5%."
        );
    }

    #[test]
    fn no_model_stops_the_asking() {
        let mut state = RowGistState::default();
        let start = Instant::now();
        let ids = state.take_request([ThreadId::new()], start).unwrap();
        state.apply_batch(
            &ids,
            state.epoch(),
            &ThreadGistBatchData {
                model: GistModelData::Disabled,
                gists: vec![],
                queued: vec![],
                in_flight: vec![],
                skipped: vec![],
            },
            start,
        );
        assert_eq!(
            state.take_request([ThreadId::new()], start + Duration::from_secs(2)),
            None
        );
        assert_eq!(state.model, Some(GistModelData::Disabled));
    }

    #[test]
    fn rows_around_the_cursor_come_cursor_first() {
        let ids: Vec<ThreadId> = (0..5).map(|_| ThreadId::new()).collect();
        let order = around(&ids, 2);
        assert_eq!(
            order,
            vec![
                ids[2].clone(),
                ids[3].clone(),
                ids[4].clone(),
                ids[1].clone(),
                ids[0].clone()
            ]
        );
    }

    #[test]
    fn an_answer_from_before_an_invalidation_is_dropped() {
        let (a, b) = (ThreadId::new(), ThreadId::new());
        let mut state = RowGistState::default();
        let start = Instant::now();
        let ids = state.take_request([a.clone(), b.clone()], start).unwrap();
        let epoch = state.epoch();
        // A new message in `a` lands while the request is out.
        state.changed([&a]);
        let batch = ThreadGistBatchData {
            model: GistModelData::Available,
            gists: vec![ready(&a, None), ready(&b, None)],
            queued: vec![],
            in_flight: vec![],
            skipped: vec![],
        };
        state.apply_batch(&ids, epoch, &batch, start);
        assert!(
            state.gists.is_empty(),
            "the old answer can't undo the invalidation"
        );
        assert_eq!(
            state.take_request([a.clone(), b.clone()], start + Duration::from_secs(1)),
            Some(vec![a.clone(), b.clone()]),
            "both rows ask again"
        );
        let epoch = state.epoch();
        state.apply_batch(&[a.clone(), b.clone()], epoch, &batch, start);
        assert_eq!(state.gists.len(), 2);

        state.invalidate_all();
        assert!(state.gists.is_empty());
    }

    #[test]
    fn state_is_capped_oldest_first() {
        let mut state = RowGistState::default();
        let ids: Vec<ThreadId> = (0..STATE_CAP + 5).map(|_| ThreadId::new()).collect();
        for id in &ids {
            state.put(&ready(id, None));
        }
        assert_eq!(state.gists.len(), STATE_CAP);
        assert!(!state.gists.contains_key(&ids[0]));
        assert!(state.gists.contains_key(ids.last().unwrap()));

        let start = Instant::now();
        for (index, chunk) in ids.chunks(50).enumerate() {
            let at = start + Duration::from_secs(index as u64);
            state.last_look = None;
            state.gists.clear();
            state.take_request(chunk.iter().cloned(), at);
        }
        assert_eq!(state.requested.len(), STATE_CAP);
    }
}
