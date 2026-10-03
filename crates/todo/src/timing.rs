//! When a to-do must be acted on, when it shows up, and when it stops
//! mattering.
//!
//! The lead time is how long the outside world takes, so it is a fixed
//! table by kind of action, not a learned guess (blueprint 22, "Lead times
//! are a fixed table by kind of action"). The window comes from code too:
//! the item's own end (an event's start, a link's expiry, a due date plus
//! grace), never from a model.

use crate::TodoKind;
use chrono::{DateTime, Datelike, Duration, Months, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};

/// What the table needs to know about one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingInput<'a> {
    pub kind: TodoKind,
    pub doc_type: Option<&'a str>,
    /// When the message arrived.
    pub arrived: DateTime<Utc>,
    pub due: Option<DateTime<Utc>>,
    /// An event's start, for RSVPs without a reply-by date.
    pub event_start: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub act_by: Option<DateTime<Utc>>,
    pub surface_at: Option<DateTime<Utc>>,
    /// How act-by was worked out, for provenance.
    pub act_by_rule: &'static str,
    /// How the surface time was worked out, for provenance.
    pub surface_rule: &'static str,
}

/// Act-by and surface time from the lead-time table. An item with no date
/// has neither: it waits in Whenever.
pub fn lead_time<Tz: TimeZone>(input: &TimingInput<'_>, tz: &Tz, morning_hour: u8) -> Timing {
    let at_once = |rule| Timing {
        act_by: input.due.or(Some(input.arrived)),
        surface_at: Some(input.arrived),
        act_by_rule: rule,
        surface_rule: "at once",
    };
    match input.kind {
        // A failed collection needs fixing now, whatever date it names.
        TodoKind::PaymentFailed => return at_once("now"),
        // Links expire in hours, so a verify link shows up at once.
        TodoKind::Verify => return at_once("the link's expiry"),
        _ => {}
    }
    let (act_by, act_by_rule) = match (input.kind, input.due) {
        (TodoKind::Rsvp, None) => match input.event_start {
            Some(start) => (start - Duration::days(2), "2 days before the event"),
            None => return Timing::undated(),
        },
        (_, None) => return Timing::undated(),
        (TodoKind::Bill, Some(due)) if input.doc_type == Some("bill_bank") => (
            minus_working_days(due, 3, tz),
            "due minus 3 working days for a bank transfer",
        ),
        (TodoKind::Document, Some(expiry))
            if matches!(input.doc_type, Some("passport" | "visa")) =>
        {
            (minus_months(expiry, 6, tz), "expiry minus 6 months")
        }
        (TodoKind::Lease, Some(end)) => (minus_months(end, 2, tz), "end minus 2 months' notice"),
        (_, Some(due)) => (due, "the due date"),
    };
    let (lead, surface_rule) = match (input.kind, input.doc_type) {
        (TodoKind::Bill, _) => (Lead::Days(3), "3 days before act-by"),
        (TodoKind::Renewal, _) => (Lead::Days(14), "14 days before act-by"),
        (TodoKind::Document, Some("passport" | "visa")) => {
            (Lead::Days(70), "10 weeks before act-by")
        }
        (TodoKind::Document, _) => (Lead::Days(56), "8 weeks before act-by"),
        (TodoKind::Lease, _) => (Lead::Days(28), "4 weeks before act-by"),
        (TodoKind::Return, _) => (Lead::Days(4), "4 days before act-by"),
        (TodoKind::Promise, _) => (Lead::WorkingDays(1), "1 working day before act-by"),
        (
            TodoKind::Rsvp
            | TodoKind::Sign
            | TodoKind::Other
            | TodoKind::PaymentFailed
            | TodoKind::Verify,
            _,
        ) => (Lead::Days(2), "2 days before act-by"),
    };
    let day = match lead {
        Lead::Days(days) => act_by.with_timezone(tz).date_naive() - Duration::days(days),
        Lead::WorkingDays(days) => minus_working_days(act_by, days, tz)
            .with_timezone(tz)
            .date_naive(),
    };
    // Never later than the arrival: a date already past shows up at once.
    let surface_at = start_of_day(day, tz, morning_hour).max(input.arrived);
    Timing {
        act_by: Some(act_by),
        surface_at: Some(surface_at),
        act_by_rule,
        surface_rule,
    }
}

enum Lead {
    Days(i64),
    WorkingDays(i64),
}

impl Timing {
    fn undated() -> Self {
        Self {
            act_by: None,
            surface_at: None,
            act_by_rule: "no date",
            surface_rule: "no date",
        }
    }
}

/// `at` moved back `months` calendar months in `tz`, so the wall time
/// stays put across a clock change.
fn minus_months<Tz: TimeZone>(at: DateTime<Utc>, months: u32, tz: &Tz) -> DateTime<Utc> {
    at.with_timezone(tz)
        .checked_sub_months(Months::new(months))
        .map_or(at, |moved| moved.with_timezone(&Utc))
}

/// `at` moved back `days` working days (Monday to Friday), keeping the
/// time of day.
pub fn minus_working_days<Tz: TimeZone>(at: DateTime<Utc>, days: i64, tz: &Tz) -> DateTime<Utc> {
    let mut moved = at;
    let mut left = days;
    while left > 0 {
        moved -= Duration::days(1);
        if !matches!(
            moved.with_timezone(tz).weekday(),
            Weekday::Sat | Weekday::Sun
        ) {
            left -= 1;
        }
    }
    moved
}

/// The start of working hours on `day` in `tz`.
pub fn start_of_day<Tz: TimeZone>(day: NaiveDate, tz: &Tz, morning_hour: u8) -> DateTime<Utc> {
    let time =
        NaiveTime::from_hms_opt(u32::from(morning_hour.min(23)), 0, 0).unwrap_or(NaiveTime::MIN);
    tz.from_local_datetime(&day.and_time(time))
        .earliest()
        .map_or_else(
            || Utc.from_utc_datetime(&day.and_time(time)),
            |local| local.with_timezone(&Utc),
        )
}

/// Where a window's end came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSource {
    Schema,
    Ics,
    Rule,
    /// A default from the table, such as a verify link's 3 days.
    Default,
    User,
}

impl WindowSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Ics => "ics",
            Self::Rule => "rule",
            Self::Default => "default",
            Self::User => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// `None` is open-ended: undated items, which the catch-up bounds.
    pub until: Option<DateTime<Utc>>,
    pub rule: &'static str,
}

/// When an item stops mattering (blueprint 22, the relevancy table).
pub fn window(input: &TimingInput<'_>) -> Window {
    let open_ended = Window {
        until: None,
        rule: "no date: open-ended",
    };
    match input.kind {
        TodoKind::Verify => Window {
            until: Some(input.due.unwrap_or(input.arrived + Duration::days(3))),
            rule: if input.due.is_some() {
                "the link's stated expiry"
            } else {
                "3 days, the usual life of a verify link"
            },
        },
        TodoKind::Rsvp => match (input.due, input.event_start) {
            (Some(reply_by), _) => Window {
                until: Some(reply_by),
                rule: "the reply-by date",
            },
            (None, Some(start)) => Window {
                until: Some(start),
                rule: "the event's start",
            },
            (None, None) => open_ended,
        },
        TodoKind::PaymentFailed => Window {
            until: Some(input.due.unwrap_or(input.arrived) + Duration::days(14)),
            rule: "14 days after it failed or fell due",
        },
        TodoKind::Bill => match input.due {
            Some(due) => Window {
                until: Some(due + Duration::days(14)),
                rule: "due plus 14 days",
            },
            None => open_ended,
        },
        TodoKind::Renewal => match input.due {
            Some(due) => Window {
                until: Some(due + Duration::days(3)),
                rule: "renewal date plus 3 days",
            },
            None => open_ended,
        },
        TodoKind::Promise | TodoKind::Sign | TodoKind::Other | TodoKind::Return => {
            match input.due {
                Some(due) => Window {
                    until: Some(due + Duration::days(7)),
                    rule: "the date plus 7 days",
                },
                None => open_ended,
            }
        }
        TodoKind::Document | TodoKind::Lease => match input.due {
            Some(due) => Window {
                until: Some(due + Duration::days(14)),
                rule: "the date plus 14 days",
            },
            None => open_ended,
        },
    }
}

/// Where a newly found item goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Open,
    /// The first run's one-time batch.
    CatchUp,
    /// Already over when found: written expired, never surfaced.
    ExpiredAtBirth(&'static str),
}

/// The first run's rules (blueprint 22, "The first run classifies newest
/// first"): nothing past its window enters; an open-ended item older than
/// the catch-up window is expired at birth. While a run is in progress,
/// what the user would meet as backlog joins the one catch-up batch
/// instead of Now: a recent undated item, or one already past its due
/// date. After the run, new items go straight in.
pub fn place(
    now: DateTime<Utc>,
    window: &Window,
    due: Option<DateTime<Utc>>,
    arrived: DateTime<Utc>,
    catchup_days: u32,
    run_in_progress: bool,
) -> Placement {
    if let Some(until) = window.until {
        return if until < now {
            Placement::ExpiredAtBirth("already over when found")
        } else if run_in_progress && due.is_some_and(|due| due < now) {
            Placement::CatchUp
        } else {
            Placement::Open
        };
    }
    if arrived < now - Duration::days(i64::from(catchup_days)) {
        Placement::ExpiredAtBirth("older than your catch-up window")
    } else if run_in_progress {
        Placement::CatchUp
    } else {
        Placement::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::London;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        London
            .with_ymd_and_hms(y, m, d, h, 0, 0)
            .single()
            .expect("valid time")
            .with_timezone(&Utc)
    }

    fn input(kind: TodoKind, due: Option<DateTime<Utc>>) -> TimingInput<'static> {
        TimingInput {
            kind,
            doc_type: None,
            arrived: at(2026, 9, 28, 8),
            due,
            event_start: None,
        }
    }

    #[test]
    fn bill_with_a_pay_link_acts_by_the_due_date_and_shows_up_three_days_before() {
        // Due Friday 9 October 2026.
        let timing = lead_time(
            &input(TodoKind::Bill, Some(at(2026, 10, 9, 23))),
            &London,
            9,
        );
        assert_eq!(timing.act_by, Some(at(2026, 10, 9, 23)));
        assert_eq!(timing.surface_at, Some(at(2026, 10, 6, 9)));
    }

    #[test]
    fn bank_transfer_bill_acts_three_working_days_early() {
        let mut bank = input(TodoKind::Bill, Some(at(2026, 10, 12, 23)));
        bank.doc_type = Some("bill_bank");
        // Monday 12 Oct minus 3 working days is Wednesday 7 Oct.
        let timing = lead_time(&bank, &London, 9);
        assert_eq!(timing.act_by, Some(at(2026, 10, 7, 23)));
        assert_eq!(timing.surface_at, Some(at(2026, 10, 4, 9)));
    }

    #[test]
    fn lead_times_per_kind() {
        let due = at(2026, 12, 31, 23);
        let surface = |kind, doc_type| {
            let mut item = input(kind, Some(due));
            item.doc_type = doc_type;
            lead_time(&item, &London, 9)
        };
        assert_eq!(
            surface(TodoKind::Renewal, None).surface_at,
            Some(at(2026, 12, 17, 9))
        );
        assert_eq!(
            surface(TodoKind::Other, None).surface_at,
            Some(at(2026, 12, 29, 9))
        );
        assert_eq!(
            surface(TodoKind::Sign, None).surface_at,
            Some(at(2026, 12, 29, 9))
        );
        // Thursday 31 Dec minus one working day is Wednesday 30 Dec.
        assert_eq!(
            surface(TodoKind::Promise, None).surface_at,
            Some(at(2026, 12, 30, 9))
        );
        let mut passport = input(TodoKind::Document, Some(at(2027, 12, 31, 23)));
        passport.doc_type = Some("passport");
        let passport = lead_time(&passport, &London, 9);
        assert_eq!(passport.act_by, Some(at(2027, 6, 30, 23)));
        assert_eq!(passport.surface_at, Some(at(2027, 4, 21, 9)));
    }

    #[test]
    fn verify_and_failed_payments_show_up_at_once() {
        let verify = lead_time(
            &input(TodoKind::Verify, Some(at(2026, 9, 29, 8))),
            &London,
            9,
        );
        assert_eq!(verify.surface_at, Some(at(2026, 9, 28, 8)));
        let failed = lead_time(&input(TodoKind::PaymentFailed, None), &London, 9);
        assert_eq!(failed.surface_at, Some(at(2026, 9, 28, 8)));
        assert_eq!(failed.act_by, Some(at(2026, 9, 28, 8)));
    }

    #[test]
    fn rsvp_without_reply_by_acts_two_days_before_the_event() {
        let mut rsvp = input(TodoKind::Rsvp, None);
        rsvp.event_start = Some(at(2026, 10, 15, 18));
        let timing = lead_time(&rsvp, &London, 9);
        assert_eq!(timing.act_by, Some(at(2026, 10, 13, 18)));
        assert_eq!(timing.surface_at, Some(at(2026, 10, 11, 9)));
        assert_eq!(window(&rsvp).until, Some(at(2026, 10, 15, 18)));
    }

    #[test]
    fn a_date_already_past_shows_up_at_arrival() {
        let timing = lead_time(
            &input(TodoKind::Bill, Some(at(2026, 9, 20, 23))),
            &London,
            9,
        );
        assert_eq!(timing.surface_at, Some(at(2026, 9, 28, 8)));
    }

    #[test]
    fn undated_items_have_no_runway_and_no_window() {
        let timing = lead_time(&input(TodoKind::Sign, None), &London, 9);
        assert_eq!((timing.act_by, timing.surface_at), (None, None));
        assert_eq!(window(&input(TodoKind::Promise, None)).until, None);
    }

    #[test]
    fn windows_per_kind() {
        let due = at(2026, 10, 9, 23);
        let until = |kind| window(&input(kind, Some(due))).until;
        assert_eq!(until(TodoKind::Bill), Some(due + Duration::days(14)));
        assert_eq!(until(TodoKind::Renewal), Some(due + Duration::days(3)));
        assert_eq!(until(TodoKind::Promise), Some(due + Duration::days(7)));
        assert_eq!(until(TodoKind::Verify), Some(due));
        assert_eq!(
            window(&input(TodoKind::Verify, None)).until,
            Some(at(2026, 10, 1, 8)),
            "3-day default"
        );
        assert_eq!(
            window(&input(TodoKind::PaymentFailed, None)).until,
            Some(at(2026, 10, 12, 8))
        );
    }

    #[test]
    fn placement_follows_the_first_run_rules() {
        let now = at(2026, 10, 2, 12);
        let closed = Window {
            until: Some(now - Duration::hours(1)),
            rule: "",
        };
        let open = Window {
            until: Some(now + Duration::days(1)),
            rule: "",
        };
        let endless = Window {
            until: None,
            rule: "",
        };
        let recent = now - Duration::days(3);
        let old = now - Duration::days(30);
        let ahead = Some(now + Duration::days(2));
        let behind = Some(now - Duration::days(2));
        assert!(matches!(
            place(now, &closed, behind, recent, 14, true),
            Placement::ExpiredAtBirth(_)
        ));
        assert_eq!(place(now, &open, ahead, old, 14, true), Placement::Open);
        assert_eq!(
            place(now, &open, behind, recent, 14, true),
            Placement::CatchUp,
            "overdue at first run is backlog"
        );
        assert_eq!(
            place(now, &open, behind, recent, 14, false),
            Placement::Open,
            "overdue after the run is news"
        );
        assert_eq!(
            place(now, &endless, None, recent, 14, true),
            Placement::CatchUp
        );
        assert_eq!(
            place(now, &endless, None, recent, 14, false),
            Placement::Open
        );
        assert_eq!(
            place(now, &endless, None, old, 14, false),
            Placement::ExpiredAtBirth("older than your catch-up window")
        );
    }
}
