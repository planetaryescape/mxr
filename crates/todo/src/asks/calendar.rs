//! Calendar asks reuse the accepted stored ICS parser and RSVP selection.
use mxr_core::{asks::*, CalendarMetadata};

pub fn include_calendar(answer: &mut AskAssessment, calendar: Option<CalendarMetadata>) {
    let Some(calendar) = calendar else {
        return;
    };
    let quote = calendar
        .summary
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            calendar
                .raw_ics
                .as_deref()
                .and_then(|ics| ics.lines().find(|line| line.contains("RSVP=TRUE")))
        })
        .unwrap_or_default();
    let quote = if quote.len() <= 2048 { quote } else { "" };
    let zone = answer.facts.zones.len();
    let tags = AskTags {
        act: Act::Request,
        actor: Actor::Owner,
        state: AskState::Pending,
        action: ActionKind::Reply,
        amount: None,
        due: None,
        reference: None,
    };
    answer.facts.zones.push(AskZone {
        kind: ZoneKind::Calendar,
        author: calendar.organizer.as_ref().map(|p| p.email.clone()),
        text: quote.into(),
    });
    answer.units.push(AskUnit {
        zone,
        span: AskSpan {
            start: 0,
            end: quote.len(),
        },
        votes: vec![AskVote {
            rule: "ics_request_rsvp_needs_action".into(),
            tags: tags.clone(),
        }],
        tags: Some(tags),
        document: DocumentReading {
            kind: DocumentKind::Other,
            status: PaymentStatus::None,
            required_fields_present: true,
        },
        answer: AskAnswer::AsksYou,
    });
    answer.pending_body = false;
    answer.calendar = Some(calendar);
}
