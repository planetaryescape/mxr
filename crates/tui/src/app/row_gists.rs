
use super::{App, MailboxView};
use mxr_core::id::ThreadId;
use mxr_core::types::LabelKind;
use mxr_protocol::{
    GistModelData, ThreadGistBatchData, ThreadGistData, ThreadGistStatusData,
    THREAD_GISTS_MAX_BATCH,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const LOOKAHEAD: usize = 40;
const LOOKBEHIND: usize = 10;
const REQUEST_TTL: Duration = Duration::from_secs(60);
const MIN_INTERVAL: Duration = Duration::from_millis(300);
const NO_MODEL_RECHECK: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowGist {
    pub about: String,
    pub ask: Option<String>,
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

    pub fn line(&self) -> &str {
        &self.line
    }
}

pub type RowGists = HashMap<ThreadId, RowGist>;

const STATE_CAP: usize = 500;

#[derive(Debug, Default)]
pub struct RowGistState {
    pub gists: RowGists,
    pub model: Option<GistModelData>,
    stored: HashMap<ThreadId, u64>,
    stored_seq: u64,
    requested: HashMap<ThreadId, Instant>,
    epoch: u64,
    off_until: Option<Instant>,
    last_look: Option<Instant>,
}

impl RowGistState {
    pub fn due(&self, now: Instant) -> bool {
        !self.off_until.is_some_and(|until| now < until)
            && !self
                .last_look
                .is_some_and(|at| now.duration_since(at) < MIN_INTERVAL)
    }

    pub fn lines_reserved(&self) -> bool {
        self.model == Some(GistModelData::Available)
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

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

    pub fn request_failed(&mut self, ids: &[ThreadId]) {
        for id in ids {
            self.requested.remove(id);
        }
    }

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

    pub fn changed<'a>(&mut self, ids: impl IntoIterator<Item = &'a ThreadId>) {
        self.epoch += 1;
        for id in ids {
            self.gists.remove(id);
            self.stored.remove(id);
            self.requested.remove(id);
        }
    }

    pub fn invalidate_all(&mut self) {
        self.epoch += 1;
        self.gists.clear();
        self.stored.clear();
        self.requested.clear();
    }
}

impl App {
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
}
