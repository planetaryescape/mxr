use chrono::{DateTime, Utc};
use mxr_core::id::ThreadId;
use mxr_store::{DeskMessage, DeskReminder, DeskReplyLater};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Timer {
    Pending,
    Back(DateTime<Utc>),
}

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

    pub(super) fn reply_later(&self, thread_id: &ThreadId, now: DateTime<Utc>) -> Option<Timer> {
        let times = self.reply_later.get(thread_id)?;
        if times.iter().any(|due| *due > now) {
            return Some(Timer::Pending);
        }
        times.iter().max().copied().map(Timer::Back)
    }

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

    fn answers(message: &DeskMessage) -> bool {
        !message.from.email.eq_ignore_ascii_case(ME)
    }

    fn message(thread: &ThreadId, from: &str, hours_ago: i64) -> DeskMessage {
        DeskMessage {
            evidence: Default::default(),
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
}
