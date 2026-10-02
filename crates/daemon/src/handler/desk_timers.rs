//! Times you set on desk conversations: reply later until a time, and
//! "bring it back if nobody replies" reminders on threads you wrote last.
//!
//! Pure rules over store rows, like the lanes. Whether a conversation is
//! away or back depends only on the clock and the stored time, never on
//! the wake loop having run, so a restart or a late tick can't lose a
//! return or show one twice.

use chrono::{DateTime, Utc};
use mxr_core::id::ThreadId;
use mxr_store::{DeskMessage, DeskReminder, DeskReplyLater};
use std::collections::HashMap;

/// Where a set time leaves a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Timer {
    /// The time is still to come: off the desk until then.
    Pending,
    /// The time came: back on the desk. Holds the time that was set.
    Back(DateTime<Utc>),
}

/// The timed reply-later flags and live reminders of one account, by
/// conversation.
#[derive(Debug, Default)]
pub(super) struct DeskTimers {
    reply_later: HashMap<ThreadId, Vec<DateTime<Utc>>>,
    reminders: HashMap<ThreadId, Vec<DeskReminder>>,
}

impl DeskTimers {
    pub(super) fn new(reply_later: Vec<DeskReplyLater>, reminders: Vec<DeskReminder>) -> Self {
        let mut timers = Self::default();
        for flag in reply_later {
            timers
                .reply_later
                .entry(flag.thread_id)
                .or_default()
                .push(flag.due_at);
        }
        for reminder in reminders {
            timers
                .reminders
                .entry(reminder.thread_id.clone())
                .or_default()
                .push(reminder);
        }
        timers
    }

    /// Reply later on the conversation: away while any of its times is
    /// still to come, back from the latest once all have passed.
    pub(super) fn reply_later(&self, thread_id: &ThreadId, now: DateTime<Utc>) -> Option<Timer> {
        let times = self.reply_later.get(thread_id)?;
        if times.iter().any(|due| *due > now) {
            return Some(Timer::Pending);
        }
        times.iter().max().copied().map(Timer::Back)
    }

    /// "Bring it back if nobody replies" on a conversation you wrote last.
    /// The reminder on your most recently stored message that has one
    /// decides, so a follow-up sent with its own time replaces an earlier
    /// return. A reminder counts only while nothing that `answers` was
    /// stored after its message: storage order, not the Date header, so a
    /// reply with a skewed clock still counts, and an answer settles it
    /// even before the reminder loop cancels it.
    pub(super) fn waiting(
        &self,
        thread: &[DeskMessage],
        answers: &dyn Fn(&DeskMessage) -> bool,
        now: DateTime<Utc>,
    ) -> Option<Timer> {
        let reminders = self.reminders.get(&thread.first()?.thread_id)?;
        let (sent_seq, reminder) = reminders
            .iter()
            .filter_map(|reminder| {
                thread
                    .iter()
                    .find(|message| message.id == reminder.sent_message_id)
                    .map(|sent| (sent.seq, reminder))
            })
            .max_by_key(|(seq, reminder)| (*seq, reminder.remind_at))?;
        if thread
            .iter()
            .any(|message| message.seq > sent_seq && answers(message))
        {
            return None;
        }
        Some(if reminder.triggered || reminder.remind_at <= now {
            Timer::Back(reminder.remind_at)
        } else {
            Timer::Pending
        })
    }

    /// Conversations a time may have brought back, so the desk can load
    /// the ones whose activity is older than its window.
    pub(super) fn maybe_back(&self, now: DateTime<Utc>) -> impl Iterator<Item = &ThreadId> {
        let reply_later = self
            .reply_later
            .iter()
            .filter(move |(_, times)| times.iter().all(|due| *due <= now))
            .map(|(thread, _)| thread);
        let reminders = self
            .reminders
            .iter()
            .filter(move |(_, reminders)| {
                reminders
                    .iter()
                    .any(|reminder| reminder.triggered || reminder.remind_at <= now)
            })
            .map(|(thread, _)| thread);
        reply_later.chain(reminders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use mxr_core::id::MessageId;
    use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};

    const ME: &str = "me@example.com";

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// Anyone but you, in these tests.
    fn answers(message: &DeskMessage) -> bool {
        !message.from.email.eq_ignore_ascii_case(ME)
    }

    fn message(thread: &ThreadId, from: &str, hours_ago: i64) -> DeskMessage {
        DeskMessage {
            id: MessageId::new(),
            seq: now().timestamp() - hours_ago * 3600,
            thread_id: thread.clone(),
            direction: if from == ME { "outbound" } else { "inbound" }.into(),
            date: now() - Duration::hours(hours_ago),
            flags: MessageFlags::READ,
            from: Address {
                name: None,
                email: from.into(),
            },
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Launch plan".into(),
            list_id: None,
            unsubscribe: UnsubscribeMethod::None,
            in_inbox: true,
            trashed: false,
            snoozed: false,
            is_invite: false,
            is_delivery: false,
        }
    }

    fn reminder(sent: &DeskMessage, remind_at: DateTime<Utc>, triggered: bool) -> DeskReminder {
        DeskReminder {
            thread_id: sent.thread_id.clone(),
            sent_message_id: sent.id.clone(),
            remind_at,
            triggered,
        }
    }

    #[test]
    fn reply_later_is_away_until_its_time_then_back() {
        let thread = ThreadId::new();
        let due = now() + Duration::days(2);
        let timers = DeskTimers::new(
            vec![DeskReplyLater {
                thread_id: thread.clone(),
                due_at: due,
            }],
            Vec::new(),
        );
        assert_eq!(timers.reply_later(&thread, now()), Some(Timer::Pending));
        assert_eq!(timers.reply_later(&thread, due), Some(Timer::Back(due)));
        assert_eq!(timers.reply_later(&ThreadId::new(), now()), None);
        assert_eq!(timers.maybe_back(now()).count(), 0);
        assert_eq!(timers.maybe_back(due).collect::<Vec<_>>(), vec![&thread]);
    }

    #[test]
    fn a_waiting_reminder_hides_until_due_and_an_answer_settles_it() {
        let thread = ThreadId::new();
        let sent = message(&thread, ME, 30);
        let due = now() + Duration::days(3);
        let timers = DeskTimers::new(Vec::new(), vec![reminder(&sent, due, false)]);
        let waiting = vec![message(&thread, "maya@example.com", 40), sent.clone()];

        assert_eq!(
            timers.waiting(&waiting, &answers, now()),
            Some(Timer::Pending)
        );
        assert_eq!(
            timers.waiting(&waiting, &answers, due),
            Some(Timer::Back(due))
        );

        let mut answered = waiting.clone();
        answered.push(message(&thread, "maya@example.com", 1));
        assert_eq!(timers.waiting(&answered, &answers, due), None);
    }

    #[test]
    fn a_follow_up_with_its_own_time_replaces_an_earlier_return() {
        let thread = ThreadId::new();
        let first = message(&thread, ME, 90);
        let follow_up = message(&thread, ME, 2);
        let fired_at = now() - Duration::hours(10);
        let fired = reminder(&first, fired_at, true);
        let conversation = [first, follow_up.clone()];

        let only_fired = DeskTimers::new(Vec::new(), vec![fired.clone()]);
        assert_eq!(
            only_fired.waiting(&conversation, &answers, now()),
            Some(Timer::Back(fired_at))
        );

        let again = DeskTimers::new(
            Vec::new(),
            vec![
                fired,
                reminder(&follow_up, now() + Duration::days(1), false),
            ],
        );
        assert_eq!(
            again.waiting(&conversation, &answers, now()),
            Some(Timer::Pending)
        );
    }
}
