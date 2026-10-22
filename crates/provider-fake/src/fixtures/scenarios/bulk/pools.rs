//! The people, senders and subjects the bulk demo draws on. The people and
//! the list, alert, promotion and spam senders are the ones the demo has
//! always had; the e2e suite, the demo rules and the marketing tapes name
//! some of them.

/// A display name and an address.
pub(crate) type Person = (&'static str, &'static str);

pub(crate) const DEMO_PEOPLE: [Person; 12] = [
    ("Maya Ortiz", "maya@orbit.example"),
    ("Theo Nash", "theo@northstar.example"),
    ("Priya Raman", "priya@atlas.example"),
    ("Jon Bell", "jon@papertrail.example"),
    ("Nora Kim", "nora@foundry.example"),
    ("Samir Patel", "samir@launchpad.example"),
    ("Elena Wood", "elena@harbor.example"),
    ("Ari Stone", "ari@fieldkit.example"),
    ("Ruth Vega", "ruth@keystone.example"),
    ("Cal Brooks", "cal@signal.example"),
    ("Iris Chen", "iris@meridian.example"),
    ("Leo Park", "leo@workbench.example"),
];

pub(crate) const NEWSLETTER_SENDERS: [Person; 4] = [
    ("Terminal Dispatch", "dispatch@lists.demo.mxr.local"),
    ("Local First Weekly", "weekly@lists.demo.mxr.local"),
    ("SQLite Notes", "notes@lists.demo.mxr.local"),
    ("Ops Digest", "ops@lists.demo.mxr.local"),
];

pub(crate) const ALERT_SENDERS: [Person; 3] = [
    ("Build Watch", "builds@alerts.demo.mxr.local"),
    ("Uptime Robot", "uptime@alerts.demo.mxr.local"),
    ("Pager Relay", "pager@alerts.demo.mxr.local"),
];

pub(crate) const PROMO_SENDERS: [Person; 4] = [
    ("Cloud Deals", "offers@promo.demo.mxr.local"),
    ("Desk Gear Outlet", "gear@promo.demo.mxr.local"),
    ("Flight Fare Drop", "fares@promo.demo.mxr.local"),
    ("Conference Passes", "events@promo.demo.mxr.local"),
];

pub(crate) const SPAM_SENDERS: [Person; 3] = [
    (
        "Account Verification",
        "verify@security-mail.demo.mxr.local",
    ),
    ("Prize Desk", "winner@claim-now.demo.mxr.local"),
    ("Payroll Notice", "payroll@notice-demo.mxr.local"),
];

pub(crate) const STORE_SENDERS: [Person; 4] = [
    ("Northwind Store", "orders@store.demo.mxr.local"),
    ("Parcel Supply Co", "no-reply@parcelsupply.demo.mxr.local"),
    ("Ridgeway Outfitters", "receipts@ridgeway.demo.mxr.local"),
    ("Lumen Electronics", "orders@lumen.demo.mxr.local"),
];

pub(crate) const BILLER_SENDERS: [Person; 3] = [
    (
        "Brightwater Utilities",
        "billing@brightwater.demo.mxr.local",
    ),
    ("Fernhill Hosting", "accounts@fernhill.demo.mxr.local"),
    ("Meridian Broadband", "billing@meridianbb.demo.mxr.local"),
];

pub(crate) const TRAVEL_SENDERS: [Person; 3] = [
    ("Skyway Airlines", "booking@skyway.demo.mxr.local"),
    ("Harbor Hotels", "reservations@harborhotels.demo.mxr.local"),
    ("Railway Tickets", "tickets@railway.demo.mxr.local"),
];

pub(crate) const SECURITY_SENDERS: [Person; 3] = [
    ("Orbit Account", "no-reply@accounts.orbit.demo.mxr.local"),
    ("Keystone ID", "security@keystone-id.demo.mxr.local"),
    (
        "Workspace Security",
        "no-reply@security.workspace.demo.mxr.local",
    ),
];

/// Subjects of the threads between people. The order is part of the demo's
/// contract: the e2e suite reads the thread at each index.
pub(crate) const SUBJECT_POOL: [&str; 20] = [
    "Launch checklist for Project Aurora",
    "Canary rollout notes",
    "Customer feedback from the design partner call",
    "Draft positioning for the CLI-first release",
    "Security review follow-up",
    "QBR prep and open questions",
    "Contract renewal details",
    "Interview panel for Staff Engineer candidate",
    "Workspace subscription seats for next quarter",
    "Flight options for the Portland demo day",
    "Incident review: delayed sync jobs",
    "Weekly local-first reading list",
    "Release branch needs a second pair of eyes",
    "Pricing page copy review",
    "API migration plan",
    "Research notes: terminal workflows",
    "Offsite agenda and logistics",
    "Spring planning: scope and owners",
    "Launch bundle: what goes in the pack",
    "Password manager rollout for the team",
];

/// The first word of a display name.
pub(crate) fn first_name(full: &str) -> &str {
    full.split_whitespace().next().unwrap_or(full)
}
