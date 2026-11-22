
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::Envelope;
use mxr_protocol::Request;

use super::MutationEffect;

pub const MUTATION_SNAPSHOT_CAPACITY: usize = 64;
pub const TRANSIENT_MUTATION_MAX_RETRIES: u8 = 3;

const MUTATION_RETRY_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const MUTATION_RETRY_MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MutationId(u64);

impl MutationId {
    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

#[derive(Debug, Default)]
pub struct MutationIdGenerator {
    next: u64,
}

impl MutationIdGenerator {
    pub fn next_id(&mut self) -> MutationId {
        let id = MutationId(self.next);
        self.next = self.next.wrapping_add(1);
        id
    }
}

#[derive(Debug, Clone)]
pub struct QueuedMutation {
    pub expected_workspace: Option<mxr_protocol::ArchiveWorkspaceData>,
    pub id: MutationId,
    pub request: Request,
    pub effect: MutationEffect,
    pub best_effort: bool,
    pub attempts: u8,
    pub run_after: Option<Instant>,
}

impl QueuedMutation {
    pub fn retry_delay(attempt: u8) -> Duration {
        let multiplier = 1u64 << attempt.saturating_sub(1).min(8);
        MUTATION_RETRY_INITIAL_BACKOFF
            .saturating_mul(multiplier as u32)
            .min(MUTATION_RETRY_MAX_BACKOFF)
    }

    pub fn ready_to_run(&self, now: Instant) -> bool {
        self.run_after.is_none_or(|run_after| run_after <= now)
    }
}

#[derive(Debug, Clone)]
pub enum MutationSnapshot {
    Flags(Vec<(MessageId, MessageFlags)>),
    Labels(Vec<(MessageId, Vec<String>)>),
    RemovedFromLists(Vec<Envelope>),
    ReplyLater(Vec<(MessageId, bool)>),
    None,
}

#[derive(Debug, Default)]
pub struct MutationSnapshotStore {
    by_id: HashMap<MutationId, MutationSnapshot>,
    insertion_order: VecDeque<MutationId>,
}

impl MutationSnapshotStore {
    pub fn insert(&mut self, id: MutationId, snapshot: MutationSnapshot) {
        if self.by_id.len() >= MUTATION_SNAPSHOT_CAPACITY {
            if let Some(oldest) = self.insertion_order.pop_front() {
                self.by_id.remove(&oldest);
            }
        }
        self.by_id.insert(id, snapshot);
        self.insertion_order.push_back(id);
    }

    pub fn take(&mut self, id: MutationId) -> Option<MutationSnapshot> {
        let snapshot = self.by_id.remove(&id)?;
        if let Some(pos) = self.insertion_order.iter().position(|other| *other == id) {
            self.insertion_order.remove(pos);
        }
        Some(snapshot)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}
