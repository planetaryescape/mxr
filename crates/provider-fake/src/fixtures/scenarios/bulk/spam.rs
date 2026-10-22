//! Families of unwanted mail: spam the provider filed under Spam, and a
//! phishing lookalike that reached the inbox.
//!
//! Visible phrasings only. The held-out ones live in `scenarios/held_out.rs`.

use super::pools::SPAM_SENDERS;
use super::{expect, Attachments, Dating, Family, Senders, Subjects, Turn, Unsubscribe, Variant};
use super::{Home, Who};

const NO_SENT: &[&str] = &[];

fn potential_spam(_: usize) -> Option<&'static str> {
    Some("potential_spam")
}

fn spam_look(_: usize) -> usize {
    10
}

fn phishing_look(_: usize) -> usize {
    12
}

// Variant i is written by SPAM_SENDERS[i % 3]: Account Verification, Prize
// Desk, then Payroll Notice.
pub(super) static SPAM_PRIZE: Family = Family {
    id: "spam-prize",
    senders: Senders::ByVariant(&SPAM_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "SP",
    style: spam_look,
    attachments: Attachments::None,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: potential_spam,
    waiting: false,
    spam_folder: true,
    archived_every: None,
    turns: &[Turn::them(
        "bulk-spam-in-spam-folder",
        "A scam the provider filed under Spam",
        expect(Who::Machine, Home::Spam, false, false),
    )
    .reviewed(
        &["who"],
        "Mail the provider filed under Spam is placed by that verdict, not by who sent it. The sender is an automated bulk address, so labelled machine; the sender kind is not what places spam.",
    )],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "verify-account",
            subjects: &["Verify your account to avoid suspension"],
            bodies: &["Dear customer,\n\nYour account will be suspended unless you verify your details today. Click here to keep your access: https://claim-now.demo.mxr.local/verify\n\nAccount Verification Team"],
        },
        Variant {
            id: "prize-won",
            subjects: &["You have won a prize! Claim now"],
            bodies: &["Congratulations! You have been selected to claim a prize worth {amount}. Click the link below within 24 hours or it goes to the next person.\n\nhttps://claim-now.demo.mxr.local/prize\n\nPrize Desk"],
        },
        Variant {
            id: "payroll-update",
            subjects: &["Payroll update: confirm your bank details"],
            bodies: &["Your next payment cannot be processed until you confirm your bank details. Use the secure link: https://notice-demo.demo.mxr.local/payroll\n\nPayroll Notice"],
        },
        Variant {
            id: "verify-pressure",
            subjects: &["Final warning: your mailbox is full"],
            bodies: &["Your mailbox is over its limit and new mail will be rejected. Free up space by signing in here: https://claim-now.demo.mxr.local/mailbox\n\nAccount Verification Team"],
        },
        Variant {
            id: "prize-gift-card",
            subjects: &["Your free gift card is waiting"],
            bodies: &["You qualify for a free gift card worth {amount}. Just complete a two-minute survey to receive it.\n\nhttps://claim-now.demo.mxr.local/gift\n\nPrize Desk"],
        },
        Variant {
            id: "payroll-bonus",
            subjects: &["Bonus payment on hold"],
            bodies: &["A bonus payment of {amount} is on hold. Reply with your bank account and sort code to release it.\n\nPayroll Notice"],
        },
    ],
};

// Variant i is written by SPAM_SENDERS[i % 3], as above.
pub(super) static PHISHING: Family = Family {
    id: "phishing-lookalike",
    senders: Senders::ByVariant(&SPAM_SENDERS),
    subjects: Subjects::PerTurn,
    observer_cc: false,
    dating: Dating::Spread,
    due_days: 0,
    ref_prefix: "PH",
    style: phishing_look,
    attachments: Attachments::None,
    brief_every_41: false,
    unsubscribe: Unsubscribe::None,
    provider_label: potential_spam,
    waiting: false,
    spam_folder: false,
    archived_every: None,
    turns: &[Turn::them(
        "bulk-phishing-in-inbox",
        "A phishing lookalike that urges you to act, which the provider left in the inbox",
        expect(Who::Machine, Home::Updates, false, false),
    )
    .reviewed(
        &["who", "home", "todo"],
        "mxr does not decide spam itself; the provider's verdict does, and this was left in the inbox, so it is placed as an automated sender's notice (Updates). Whether suspicious mail should be held back is a policy call. The firm part is that urgent wording from a lookalike must not become a to-do (blueprint 22, N2 to N4 are for genuine alerts).",
    )],
    sent_replies: NO_SENT,
    variants: &[
        Variant {
            id: "unusual-sign-in",
            subjects: &["Action required: unusual sign-in attempt"],
            bodies: &["We detected an unusual sign-in attempt on your account. To protect it, confirm your identity now or access will be limited.\n\nVerify now: https://security-mail.demo.mxr.local/reset\n\nAccount Verification"],
        },
        Variant {
            id: "prize-claim",
            subjects: &["Action required: claim your reward before it expires"],
            bodies: &["Your reward is about to expire. Act now to keep it: confirm your delivery address at the link below.\n\nhttps://claim-now.demo.mxr.local/reward\n\nPrize Desk"],
        },
        Variant {
            id: "payroll-reset",
            subjects: &["Urgent password reset notice"],
            bodies: &["Your payroll account password has expired. Complete the urgent password reset today to avoid a delay to your payment.\n\nhttps://notice-demo.demo.mxr.local/reset\n\nPayroll Notice"],
        },
        Variant {
            id: "locked-account",
            subjects: &["Your account has been locked"],
            bodies: &["Several failed attempts locked your account. Urgent password reset required: sign in through the secure page below within one hour.\n\nhttps://security-mail.demo.mxr.local/unlock\n\nAccount Verification"],
        },
        Variant {
            id: "winner-form",
            subjects: &["Action required: winner details needed"],
            bodies: &["You are a winner. Action required: send us your full name, date of birth and card number so we can deliver your prize.\n\nPrize Desk"],
        },
        Variant {
            id: "payroll-details",
            subjects: &["Payroll: action required on your direct deposit"],
            bodies: &["Direct deposit for your next payment is paused. Action required: log in with your employee ID and password to re-enable it.\n\nhttps://notice-demo.demo.mxr.local/deposit\n\nPayroll Notice"],
        },
    ],
};
