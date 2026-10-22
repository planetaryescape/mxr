//! Families of notices from machines: a booking with its reminders, a service
//! alert and an account's security notice. Their messages are Updates; a
//! booking is also kept as a record.
//!
//! Visible phrasings only. The held-out ones live in `scenarios/held_out.rs`.

use super::pools::{ALERT_SENDERS, SECURITY_SENDERS, TRAVEL_SENDERS};
use super::{
    Attachments, Dating, Family, Senders, Subjects, Turn, Unsubscribe, Variant, FILED, NOTICE,
};

const NO_SENT: &[&str] = &[];

/// Why the record answer of a trip reminder or a schedule change is a
/// judgement call.
const TRIP_NOTICE_WHY: &str = "Blueprint 22 joins an order's dispatch and delivery to its record and files a trip's bookings as one trip; it does not say whether a reminder or a schedule change that names the booking joins the trip. Labelled no record: the email adds no booking, amount or document.";

fn travel(_: usize) -> Option<&'static str> {
    Some("travel")
}

fn alerts(_: usize) -> Option<&'static str> {
    Some("alerts")
}

fn none(_: usize) -> Option<&'static str> {
    None
}

fn travel_look(_: usize) -> usize {
    6
}

fn alert_look(_: usize) -> usize {
    7
}

fn plain(_: usize) -> usize {
    0
}

pub(super) static TRAVEL_BOOKING: Family = Family {
    id: "travel-booking",
    senders: Senders::ByVariant(&TRAVEL_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    reply_headers: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "MXR",
    style: travel_look,
    attachments: Attachments::FirstTurn,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: travel,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[
        Turn::them(
            "bulk-travel-booking",
            "A booking confirmation with the reference and the itinerary",
            FILED,
        ),
        Turn::them(
            "bulk-travel-payment",
            "The payment receipt for the same booking",
            FILED,
        ),
        Turn::them(
            "bulk-travel-reminder",
            "A reminder that the trip is coming up, with the details",
            NOTICE,
        )
        .reviewed(&["record"], TRIP_NOTICE_WHY),
        Turn::them(
            "bulk-travel-change",
            "A small change to the schedule, nothing to decide",
            NOTICE,
        )
        .reviewed(&["record"], TRIP_NOTICE_WHY),
        Turn::them(
            "bulk-travel-survey",
            "A request to rate the trip: a notification, nothing to keep",
            NOTICE,
        ),
    ],
    sent_replies: NO_SENT,
    // Variant i is written by TRAVEL_SENDERS[i % 3]: an airline, a hotel,
    // then a railway.
    variants: &[
        Variant {
            id: "airline-confirmation",
            subjects: &[
                "Your booking is confirmed ({ref})",
                "Payment received for booking {ref}",
                "Your flight is coming up",
                "Your flight time has changed slightly",
                "How was your flight?",
            ],
            bodies: &[
                "Thank you for flying with {org}.\n\nBooking reference: {ref}\nBooked on: {date}\nPassenger: Alex Demo\n\nYour itinerary is attached. We'll send a reminder before you travel.",
                "We have received your payment of {amount} for booking {ref}. Your receipt is below.\n\nTotal paid: {amount}\nPaid on: {date}",
                "Your trip on booking {ref} is coming up. Your itinerary is in your account, and your baggage allowance is shown on it.",
                "The departure time for booking {ref} has moved by ten minutes. Your new times are in your account. You do not need to do anything.",
                "Thanks for flying with {org}. Tell us about your flight on booking {ref}.",
            ],
        },
        Variant {
            id: "hotel-reservation",
            subjects: &[
                "Your reservation at {org}",
                "Receipt for reservation {ref}",
                "Your stay is nearly here",
                "A note about your room",
                "How was your stay?",
            ],
            bodies: &[
                "Your reservation is confirmed.\n\nReservation: {ref}\nBooked on: {date}\nRoom: Double, city view\n\nWe look forward to welcoming you. Details are attached.",
                "Thank you for your payment of {amount} for reservation {ref}. This email is your receipt.",
                "Your stay at {org} is coming up soon. Check-in is from the afternoon and the front desk is open all day.",
                "We've moved you to a quieter room on the same floor for reservation {ref}. Nothing changes about your booking.",
                "We hope you enjoyed your stay with {org}. Share your thoughts in our short guest survey.",
            ],
        },
        Variant {
            id: "rail-tickets",
            subjects: &[
                "Your tickets ({ref})",
                "Your receipt for booking {ref}",
                "Your journey is coming up",
                "Platform change for your journey",
                "Rate your journey",
            ],
            bodies: &[
                "Your tickets are ready.\n\nBooking: {ref}\nBought on: {date}\nSeats reserved: 2\n\nYour itinerary is attached. Show the barcode at the gate.",
                "Payment of {amount} received for booking {ref}. Thank you.",
                "Your journey on booking {ref} is coming up. Remember to bring photo ID.",
                "The platform for your outbound journey has changed. Check the departure boards on the day. Your tickets are unaffected.",
                "How was your journey with {org}? We'd love your feedback.",
            ],
        },
        Variant {
            id: "airline-short",
            subjects: &[
                "Booking {ref}: e-ticket",
                "Booking {ref}: receipt",
                "Trip reminder: booking {ref}",
                "Schedule update: booking {ref}",
                "Flight feedback",
            ],
            bodies: &[
                "E-ticket for booking {ref}.\nDate booked: {date}\nFare paid: {amount}\nItinerary attached.",
                "Receipt for booking {ref}: {amount}, paid {date}.",
                "Reminder: your trip on booking {ref} is coming up. Itinerary in your account.",
                "Schedule update for booking {ref}: minor change to departure time. No action required.",
                "Please rate your recent flight.",
            ],
        },
    ],
};

pub(super) static SERVICE_ALERT: Family = Family {
    id: "service-alert",
    senders: Senders::ByVariant(&ALERT_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    reply_headers: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "INC",
    style: alert_look,
    attachments: Attachments::None,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: alerts,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[Turn::them(
        "bulk-service-alert",
        "A monitoring or build alert about a service: a notification, nothing to do",
        NOTICE,
    )],
    sent_replies: NO_SENT,
    // Variant i is written by ALERT_SENDERS[i % 3]: a build bot, an uptime
    // monitor, then a pager.
    variants: &[
        Variant {
            id: "build-failed",
            subjects: &["Build failed on release branch"],
            bodies: &["Build #{n}41 on release/2.4 failed at the test stage.\n\n3 tests failed in sync_engine. Full log: https://ci.demo.mxr.local/builds/{ref}\n\nBuild Watch"],
        },
        Variant {
            id: "monitor-down",
            subjects: &["Monitor is down: api.demo.mxr.local"],
            bodies: &["Your monitor api.demo.mxr.local stopped responding on {date}. We'll keep checking and tell you when it recovers.\n\nUptime Robot"],
        },
        Variant {
            id: "incident-opened",
            subjects: &["Incident opened: delayed sync jobs"],
            bodies: &["Incident {ref} was opened for delayed sync jobs. Status: investigating. Severity: minor. Updates will follow in the incident channel.\n\nPager Relay"],
        },
        Variant {
            id: "build-passed",
            subjects: &["Build passed on main"],
            bodies: &["Build #{n}12 on main passed all {n} checks in 6 minutes. Details: https://ci.demo.mxr.local/builds/{ref}\n\nBuild Watch"],
        },
        Variant {
            id: "monitor-recovered",
            subjects: &["Monitor recovered: api.demo.mxr.local"],
            bodies: &["api.demo.mxr.local is responding again after a short outage on {date}. Total downtime: {n} minutes.\n\nUptime Robot"],
        },
        Variant {
            id: "incident-resolved",
            subjects: &["Incident resolved: delayed sync jobs"],
            bodies: &["Incident {ref} has been resolved. The first alert fired at 02:14 and recovery was confirmed at 02:31. A review is scheduled.\n\nPager Relay"],
        },
    ],
};

pub(super) static SECURITY_NOTICE: Family = Family {
    id: "security-notice",
    senders: Senders::Pool(&SECURITY_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    reply_headers: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "SEC",
    style: plain,
    attachments: Attachments::None,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: none,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[Turn::them(
        "bulk-security-notice",
        "An account provider's sign-in or security alert that says no action is needed if it was you",
        NOTICE,
    )
    .reviewed(
        &["todo"],
        "updates_demo.rs suggests a to-do for a sign-in alert, but the mail says no action is needed if it was you, so it is labelled no to-do. Same call as updates-new-sign-in.",
    )],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "new-sign-in",
            subjects: &["New sign-in to your account"],
            bodies: &["We noticed a new sign-in to your account on {date} from a Mac in London.\n\nIf this was you, you don't need to do anything. If it wasn't, secure your account: https://accounts.demo.mxr.local/security/{ref}"],
        },
        Variant {
            id: "password-changed",
            subjects: &["Your password was changed"],
            bodies: &["The password for your account was changed on {date}. If you made this change, there is nothing more to do.\n\nIf you didn't, reset it from your account settings: https://accounts.demo.mxr.local/security/{ref}"],
        },
        Variant {
            id: "two-step-on",
            subjects: &["Two-step verification turned on"],
            bodies: &["Two-step verification is now on for your account. You'll be asked for a code when you sign in on a new device.\n\nIf you didn't turn this on, review your security settings: https://accounts.demo.mxr.local/security/{ref}"],
        },
        Variant {
            id: "recovery-email",
            subjects: &["Recovery email updated"],
            bodies: &["The recovery email on your account was updated on {date}. We're telling you so you know it was done.\n\nNot you? Visit https://accounts.demo.mxr.local/security/{ref}"],
        },
        Variant {
            id: "new-device",
            subjects: &["A new device signed in to your account"],
            bodies: &["A new device (a phone running the latest app) signed in to your account on {date}. No action is needed if this was you.\n\nSecurity settings: https://accounts.demo.mxr.local/security/{ref}"],
        },
        Variant {
            id: "workspace-sign-in",
            subjects: &["Security alert for your workspace"],
            bodies: &["Someone signed in to your workspace from a new location on {date}. If that was you, ignore this message.\n\nReview recent sign-ins: https://accounts.demo.mxr.local/security/{ref}"],
        },
    ],
};
