//! Families of mail from lists: newsletters and promotions. Both carry an
//! unsubscribe, and both are Reading.
//!
//! Visible phrasings only. The held-out ones live in `scenarios/held_out.rs`.

use super::pools::{NEWSLETTER_SENDERS, PROMO_SENDERS};
use super::{Attachments, Dating, Family, Senders, Subjects, Turn, Unsubscribe, Variant, READING};

const NO_SENT: &[&str] = &[];

fn newsletters(_: usize) -> Option<&'static str> {
    Some("newsletters")
}

fn promotions(_: usize) -> Option<&'static str> {
    Some("promotions")
}

fn newsletter_look(_: usize) -> usize {
    2
}

fn promo_look(_: usize) -> usize {
    11
}

// Variant i is written by NEWSLETTER_SENDERS[i % 4]: Terminal Dispatch,
// Local First Weekly, SQLite Notes, then Ops Digest.
pub(super) static NEWSLETTER: Family = Family {
    id: "newsletter",
    senders: Senders::ByVariant(&NEWSLETTER_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "NL",
    style: newsletter_look,
    attachments: Attachments::EveryNth(8),
    brief_every_41: false,
    unsubscribe: Unsubscribe::Lists,
    provider_label: newsletters,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[Turn::them(
        "bulk-newsletter-issue",
        "An issue of a newsletter with an unsubscribe: Reading",
        READING,
    )],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "dispatch-shell",
            subjects: &["Terminal Dispatch #{n}: shell tricks, a new prompt, and one good essay"],
            bodies: &["This week in the terminal.\n\n1. A prompt that shows the one thing you need\n2. Why your shell history deserves a database\n3. Essay: the case for boring tools\n\nRead online: https://lists.demo.mxr.local/dispatch/{n}\n\nYou are receiving this because you subscribed. Unsubscribe any time."],
        },
        Variant {
            id: "weekly-sync",
            subjects: &["Local First Weekly: sync without a server"],
            bodies: &["Local First Weekly\n\nThis week: how two devices agree without a central server, a look at conflict-free data types, and links worth saving.\n\nTop links:\n- https://lists.demo.mxr.local/articles/local-first-mail\n- https://lists.demo.mxr.local/articles/crdt-primer\n\nManage preferences or unsubscribe at the bottom of this email."],
        },
        Variant {
            id: "notes-wal",
            subjects: &["SQLite Notes: WAL mode, explained"],
            bodies: &["SQLite Notes\n\nWrite-ahead logging lets readers and a writer work at once. This issue covers when WAL helps, when it doesn't, and the checkpoint settings people get wrong.\n\nFull article: https://sqlite.org/wal.html\n\nTo unsubscribe, reply with the word unsubscribe."],
        },
        Variant {
            id: "ops-incidents",
            subjects: &["Ops Digest: this month in incidents"],
            bodies: &["Ops Digest\n\nThree postmortems worth your time, a checklist for on-call handovers, and a short piece on alert fatigue.\n\nhttps://lists.demo.mxr.local/ops/{n}\n\nUnsubscribe from this digest at any time."],
        },
        Variant {
            id: "dispatch-release",
            subjects: &["Terminal Dispatch #{n}: what to read before your next release"],
            bodies: &["Terminal Dispatch\n\nA short issue this week: a release checklist that fits on a postcard, two tools for diffing config, and a reader's story about a deploy that went right.\n\nOpen in browser: https://lists.demo.mxr.local/dispatch/{n}\n\nYou can unsubscribe with one click."],
        },
        Variant {
            id: "weekly-links",
            subjects: &["Local First Weekly: ten links on offline apps"],
            bodies: &["Local First Weekly\n\nTen links on building apps that keep working offline, with a line on each.\n\n1. Why offline-first is a product decision\n2. Syncing a to-do list the hard way\n3. What a good empty state says when you're offline\n\nUnsubscribe or change how often we write."],
        },
    ],
};

// Variant i is written by PROMO_SENDERS[i % 4]: Cloud Deals, Desk Gear
// Outlet, Flight Fare Drop, then Conference Passes.
pub(super) static PROMOTION: Family = Family {
    id: "promotion",
    senders: Senders::ByVariant(&PROMO_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    dating: Dating::Spread,
    due_days: 5,
    ref_prefix: "PR",
    style: promo_look,
    attachments: Attachments::EveryNth(8),
    brief_every_41: false,
    unsubscribe: Unsubscribe::Promo,
    provider_label: promotions,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[Turn::them(
        "bulk-promotion",
        "A company's promotion with an unsubscribe: a list, not spam",
        READING,
    )
    .reviewed(
        &["home"],
        "twins.rs labels a promotion with List-Unsubscribe as Reading (marketing-with-an-amount). Blueprint 22 lists a sale or offer among Updates' facts with a seven-day window and never a record. Home is a judgement call between the two; labelled Reading to match the twin.",
    )],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "cloud-upgrade",
            subjects: &["Spring upgrade offer for your workspace"],
            bodies: &["Hi Alex,\n\nUpgrade your workspace and save 30% on the annual plan. The offer ends {due}.\n\nView promotion: https://promo.demo.mxr.local/offers/workspace\n\nYou are receiving this because you signed up for offers. Unsubscribe any time."],
        },
        Variant {
            id: "gear-bundle",
            subjects: &["Limited-time launch bundle"],
            bodies: &["The desk bundle is back: a monitor arm, a lamp and a cable kit for {amount}. While stocks last.\n\nShop the bundle: https://promo.demo.mxr.local/offers/bundle\n\nUnsubscribe from promotional emails."],
        },
        Variant {
            id: "fare-drop",
            subjects: &["Fares to Portland from {amount}"],
            bodies: &["Flights to Portland have dropped. Return fares start from {amount} for travel next month.\n\nSee fares: https://promo.demo.mxr.local/offers/fares\n\nTo stop receiving fare alerts, unsubscribe."],
        },
        Variant {
            id: "conference-early-bird",
            subjects: &["Early-bird passes end soon"],
            bodies: &["Early-bird conference passes end on {due}. Join {n} hundred builders for two days of talks.\n\nGet your pass: https://promo.demo.mxr.local/offers/pass\n\nUnsubscribe from event emails."],
        },
        Variant {
            id: "cloud-storage",
            subjects: &["Try extra storage free for 30 days"],
            bodies: &["Run out of space? Try an extra terabyte free for 30 days, then {amount} a month. Cancel any time.\n\nStart your free month: https://promo.demo.mxr.local/offers/storage\n\nUnsubscribe from marketing emails."],
        },
        Variant {
            id: "gear-sale",
            subjects: &["Weekend sale: 20% off monitor arms"],
            bodies: &["Our weekend sale is on. 20% off monitor arms and desk shelves until {due}.\n\nShop the sale: https://promo.demo.mxr.local/offers/sale\n\nYou can unsubscribe at any time."],
        },
    ],
};
