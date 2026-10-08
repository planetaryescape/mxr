//! Subscriptions: charges from one issuer for one product that come at a
//! regular cadence.
//!
//! Detection works the way bank apps do it (Plaid's recurring streams,
//! Rocket Money, Monzo): group the payments by merchant and product, look
//! for a steady interval, and only call it recurring after enough
//! observations. Here the payments are Archive's receipts and invoices.
//!
//! * A charge's product comes from its title ([`product`]); a title that
//!   names no product falls back to the amount, so two plans from one
//!   issuer stay apart.
//! * A subscription needs three charges at a weekly, monthly, quarterly or
//!   yearly interval within a tolerance (a month is 30 days, give or take
//!   5), with one missed cycle allowed between two charges. A yearly one
//!   counts from two charges and is unconfirmed until the third.
//! * Amounts must hold steady, allowing step changes that stick (a price
//!   change), unless schema.org or the user stated the plan.
//! * Charges from the same issuer that fit no subscription are one-offs.
//!
//! Everything is recomputed from the records on each read, ordered by day
//! and record id, so the result never depends on storage order and never
//! goes stale.

mod load;
pub mod product;

use crate::fields::day_at;
use crate::RecordKind;
use chrono::{DateTime, Duration, Months, NaiveDate, TimeZone, Timelike, Utc};
use std::collections::BTreeMap;

pub use load::{load, Loaded};
pub use product::is_cancellation;

/// One receipt or invoice, as detection reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeInput {
    pub record_id: String,
    pub kind: RecordKind,
    pub issuer_key: String,
    pub issuer: Option<String>,
    pub title: Option<String>,
    /// The title came from schema.org or the user: a stated plan.
    pub title_stated: bool,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    /// The ledger date.
    pub at: DateTime<Utc>,
    /// Every money and date field came from schema.org or the user.
    pub checked: bool,
}

/// An email from an issuer's sender that says a subscription ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationInput {
    pub issuer_key: String,
    pub subject: String,
    pub at: DateTime<Utc>,
    pub message_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cadence {
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl Cadence {
    pub const ALL: [Self; 4] = [Self::Weekly, Self::Monthly, Self::Quarterly, Self::Yearly];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }

    /// "a week", for "about a week apart".
    pub fn interval(self) -> &'static str {
        match self {
            Self::Weekly => "a week",
            Self::Monthly => "a month",
            Self::Quarterly => "three months",
            Self::Yearly => "a year",
        }
    }

    fn period_days(self) -> f64 {
        match self {
            Self::Weekly => 7.0,
            Self::Monthly => 30.44,
            Self::Quarterly => 91.31,
            Self::Yearly => 365.25,
        }
    }

    /// How far a charge may land from its expected day.
    fn tolerance_days(self) -> f64 {
        match self {
            Self::Weekly => 1.5,
            Self::Monthly => 5.0,
            Self::Quarterly => 10.0,
            Self::Yearly => 20.0,
        }
    }

    /// How long after the expected day a charge counts as missed.
    pub fn grace_days(self) -> i64 {
        match self {
            Self::Weekly => 3,
            Self::Monthly => 7,
            Self::Quarterly => 14,
            Self::Yearly => 30,
        }
    }

    pub fn per_year(self) -> i64 {
        match self {
            Self::Weekly => 52,
            Self::Monthly => 12,
            Self::Quarterly => 4,
            Self::Yearly => 1,
        }
    }

    /// `day` plus `n` cycles. Months keep the day of the month, clamped
    /// to the month's end.
    pub fn add(self, day: NaiveDate, n: u32) -> NaiveDate {
        let months = match self {
            Self::Weekly => return day + Duration::days(7 * i64::from(n)),
            Self::Monthly => n,
            Self::Quarterly => 3 * n,
            Self::Yearly => 12 * n,
        };
        day.checked_add_months(Months::new(months)).unwrap_or(day)
    }

    /// How many cycles `gap` days are, when it is a whole number of them
    /// (one or two) within the tolerance.
    fn cycles(self, gap: i64) -> Option<u32> {
        let period = self.period_days();
        let k = (gap as f64 / period).round();
        if !(1.0..=2.0).contains(&k) {
            return None;
        }
        let allowed = self.tolerance_days() * if k > 1.0 { 1.5 } else { 1.0 };
        ((gap as f64 - k * period).abs() <= allowed).then_some(k as u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Active,
    /// The expected charge is past its grace period.
    Overdue,
    /// Two cycles missed, or a cancellation email after the last charge.
    Ended,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Overdue => "overdue",
            Self::Ended => "ended",
        }
    }
}

/// One payment: a record, or a receipt and an invoice for the same charge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge {
    pub record_ids: Vec<String>,
    pub day: NaiveDate,
    pub amount_minor: Option<i64>,
    pub checked: bool,
}

impl Charge {
    pub fn at(&self) -> DateTime<Utc> {
        day_at(self.day)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceChange {
    pub day: NaiveDate,
    /// The first charge at the new price.
    pub record_id: String,
    pub from_minor: i64,
    pub to_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cancellation {
    pub day: NaiveDate,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    /// `sub_` and a hash of the key: stable while the first charge stays.
    pub id: String,
    pub key: String,
    pub issuer_key: String,
    pub issuer: String,
    /// The product as a key, empty when the amount tells it apart.
    pub product_key: String,
    /// "Premium", when the title names it.
    pub product: Option<String>,
    pub product_stated: bool,
    pub cadence: Cadence,
    pub currency: Option<String>,
    /// The newest charge's amount.
    pub amount_minor: Option<i64>,
    pub amount_checked: bool,
    /// Oldest first.
    pub charges: Vec<Charge>,
    pub price_changes: Vec<PriceChange>,
    /// Gaps of exactly one cycle, and of two (a missed charge between).
    pub single_gaps: u32,
    pub double_gaps: u32,
    pub status: Status,
    /// "Charged about a month apart since March 2025", "No charge since
    /// 3 Aug; 2 expected charges missed", "Cancellation email on 3 Oct".
    pub status_reason: String,
    pub next_expected: Option<NaiveDate>,
    pub cancellation: Option<Cancellation>,
}

impl Subscription {
    pub fn start(&self) -> NaiveDate {
        self.charges.first().map_or(NaiveDate::MIN, |c| c.day)
    }

    pub fn last_charge(&self) -> &Charge {
        // A subscription is never built with fewer than two charges.
        &self.charges[self.charges.len() - 1]
    }

    /// Three or more charges; a yearly one seen twice is not yet.
    pub fn confirmed(&self) -> bool {
        self.charges.len() >= 3
    }

    /// Every charge's date came from schema.org or the user.
    pub fn dates_checked(&self) -> bool {
        self.charges.iter().all(|c| c.checked)
    }

    /// "Spotify Premium", or the issuer alone.
    pub fn title(&self) -> String {
        match &self.product {
            Some(product) => format!("{} {product}", self.issuer),
            None => self.issuer.clone(),
        }
    }

    pub fn yearly_cost_minor(&self) -> Option<i64> {
        self.amount_minor.map(|a| a * self.cadence.per_year())
    }

    pub fn record_ids(&self) -> impl Iterator<Item = &str> {
        self.charges
            .iter()
            .flat_map(|c| c.record_ids.iter().map(String::as_str))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detection {
    /// Ended last, then by issuer and product.
    pub subscriptions: Vec<Subscription>,
    /// Per issuer with a subscription: its receipts and invoices that are
    /// not part of one.
    pub one_offs: BTreeMap<String, Vec<String>>,
}

/// Amounts within this fraction are the same price (card FX and rounding).
const SAME_PRICE: f64 = 0.02;
/// Charges this close with the same amount are one payment.
const SAME_CHARGE_DAYS: i64 = 3;

/// The day a charge happened in the user's zone. A day stored as midday
/// UTC (`day_at`) is already a day.
pub fn local_day<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> NaiveDate {
    if at.hour() == 12 && at.minute() == 0 && at.second() == 0 {
        at.date_naive()
    } else {
        at.with_timezone(tz).date_naive()
    }
}

fn same_price(a: Option<i64>, b: Option<i64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let base = a.abs().max(b.abs()).max(1) as f64;
            ((a - b).abs() as f64) / base <= SAME_PRICE
        }
        (None, None) => true,
        _ => false,
    }
}

/// A charge before chaining.
#[derive(Debug, Clone)]
struct Point<'a> {
    input: &'a ChargeInput,
    day: NaiveDate,
}

/// A run of charges at one cadence, before validation.
#[derive(Debug, Clone)]
struct Chain {
    cadence: Cadence,
    charges: Vec<Charge>,
    single: u32,
    double: u32,
    /// Other charges at this price inside the run's span: many means
    /// frequent purchases (a daily coffee), not a subscription.
    crowd: usize,
}

pub fn detect<Tz: TimeZone>(
    inputs: &[ChargeInput],
    cancellations: &[CancellationInput],
    today: NaiveDate,
    tz: &Tz,
) -> Detection {
    // issuer -> (product key, currency) -> charges, ordered by day then id
    // so the result never depends on the order records came in.
    type Bucket<'a> = BTreeMap<(String, String), Vec<Point<'a>>>;
    let mut by_issuer: BTreeMap<&str, Bucket<'_>> = BTreeMap::new();
    // The newest charge names the issuer and the product; ties go to the
    // larger record id, so the name never depends on input order.
    let mut names: BTreeMap<&str, (NaiveDate, &str, &str)> = BTreeMap::new();
    let mut products: BTreeMap<(&str, String), (NaiveDate, &str, String, bool)> = BTreeMap::new();
    for input in inputs {
        if !matches!(input.kind, RecordKind::Receipt | RecordKind::Invoice)
            || input.issuer_key.is_empty()
        {
            continue;
        }
        let day = local_day(input.at, tz);
        let product = product::product(input.title.as_deref(), input.issuer.as_deref());
        let product_key = product.as_ref().map(|p| p.1.clone()).unwrap_or_default();
        let id = input.record_id.as_str();
        if let Some(issuer) = input.issuer.as_deref() {
            let name = names
                .entry(input.issuer_key.as_str())
                .or_insert((day, id, issuer));
            if (day, id) >= (name.0, name.1) {
                *name = (day, id, issuer);
            }
        }
        if let Some((shown, key)) = product {
            let entry = products.entry((input.issuer_key.as_str(), key)).or_insert((
                day,
                id,
                shown.clone(),
                input.title_stated,
            ));
            let stated = entry.3 || input.title_stated;
            if (day, id) >= (entry.0, entry.1) {
                *entry = (day, id, shown, stated);
            } else {
                entry.3 = stated;
            }
        }
        by_issuer
            .entry(input.issuer_key.as_str())
            .or_default()
            .entry((product_key, input.currency.clone().unwrap_or_default()))
            .or_default()
            .push(Point { input, day });
    }

    let mut out = Detection::default();
    for (issuer_key, buckets) in by_issuer {
        let issuer = names
            .get(issuer_key)
            .map_or_else(|| issuer_key.to_string(), |n| n.2.to_string());
        let mut found: Vec<Subscription> = Vec::new();
        let mut all_ids: Vec<&str> = Vec::new();
        for ((product_key, currency), mut points) in buckets {
            points.sort_by(|a, b| {
                a.day
                    .cmp(&b.day)
                    .then_with(|| a.input.record_id.cmp(&b.input.record_id))
            });
            all_ids.extend(points.iter().map(|p| p.input.record_id.as_str()));
            let stated_product = products.get(&(issuer_key, product_key.clone()));
            let stated = stated_product.is_some_and(|p| p.3);
            let groups = if product_key.is_empty() {
                bands(&points)
            } else {
                vec![points]
            };
            let mut chains: Vec<Chain> = Vec::new();
            for band in groups {
                chains.extend(extract_chains(collapse(&band)));
            }
            for chain in stitch(chains) {
                if !valid(&chain, stated) {
                    continue;
                }
                found.push(build(
                    issuer_key,
                    &issuer,
                    &product_key,
                    stated_product.map(|p| p.2.clone()),
                    stated,
                    &currency,
                    chain,
                ));
            }
        }
        if found.is_empty() {
            continue;
        }
        apply_cancellations(&mut found, issuer_key, cancellations, tz);
        for subscription in &mut found {
            set_status(subscription, today);
        }
        let members: std::collections::HashSet<&str> =
            found.iter().flat_map(Subscription::record_ids).collect();
        let mut one_offs: Vec<String> = all_ids
            .into_iter()
            .filter(|id| !members.contains(id))
            .map(str::to_string)
            .collect();
        one_offs.sort();
        if !one_offs.is_empty() {
            out.one_offs.insert(issuer_key.to_string(), one_offs);
        }
        out.subscriptions.extend(found);
    }
    sort(&mut out.subscriptions);
    out
}

/// Live ones first, then by issuer and product.
fn sort(subscriptions: &mut [Subscription]) {
    subscriptions.sort_by(|a, b| {
        (a.status == Status::Ended)
            .cmp(&(b.status == Status::Ended))
            .then_with(|| a.issuer.to_lowercase().cmp(&b.issuer.to_lowercase()))
            .then_with(|| a.product.cmp(&b.product))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Splits charges with no product by amount: a band holds amounts within
/// [`SAME_PRICE`] of its smallest, so a shop's many prices never chain into
/// one wide band. No amount is its own band.
fn bands<'a>(points: &[Point<'a>]) -> Vec<Vec<Point<'a>>> {
    let mut sorted: Vec<&Point<'a>> = points.iter().collect();
    sorted.sort_by(|a, b| {
        a.input
            .amount_minor
            .cmp(&b.input.amount_minor)
            .then_with(|| a.day.cmp(&b.day))
            .then_with(|| a.input.record_id.cmp(&b.input.record_id))
    });
    let mut out: Vec<Vec<Point<'a>>> = Vec::new();
    let mut anchor: Option<Option<i64>> = None;
    for point in sorted {
        let amount = point.input.amount_minor;
        let joins = anchor.is_some_and(|first| same_price(first, amount));
        match out.last_mut() {
            Some(band) if joins => band.push(point.clone()),
            _ => {
                out.push(vec![point.clone()]);
                anchor = Some(amount);
            }
        }
    }
    for band in &mut out {
        band.sort_by(|a, b| {
            a.day
                .cmp(&b.day)
                .then_with(|| a.input.record_id.cmp(&b.input.record_id))
        });
    }
    out
}

/// Folds a receipt and an invoice for the same payment into one charge.
fn collapse(points: &[Point<'_>]) -> Vec<Charge> {
    let mut out: Vec<Charge> = Vec::new();
    for point in points {
        if let Some(last) = out.last_mut() {
            if (point.day - last.day).num_days() <= SAME_CHARGE_DAYS
                && same_price(last.amount_minor, point.input.amount_minor)
            {
                last.record_ids.push(point.input.record_id.clone());
                last.checked &= point.input.checked;
                continue;
            }
        }
        out.push(Charge {
            record_ids: vec![point.input.record_id.clone()],
            day: point.day,
            amount_minor: point.input.amount_minor,
            checked: point.input.checked,
        });
    }
    out
}

/// The longest run at one cadence, then the longest in what is left, and
/// so on: two identical plans from one issuer on different days are two
/// runs.
fn extract_chains(mut charges: Vec<Charge>) -> Vec<Chain> {
    let mut chains = Vec::new();
    while charges.len() >= 2 {
        let best = Cadence::ALL
            .iter()
            .filter_map(|cadence| longest(&charges, *cadence))
            .max_by(|a, b| {
                (a.1.len(), a.2, std::cmp::Reverse(a.3))
                    .cmp(&(b.1.len(), b.2, std::cmp::Reverse(b.3)))
                    .then_with(|| b.0.cmp(&a.0))
            });
        let Some((cadence, picked, single, double)) = best else {
            break;
        };
        if picked.len() < 2 {
            break;
        }
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        for (index, charge) in charges.into_iter().enumerate() {
            if picked.contains(&index) {
                taken.push(charge);
            } else {
                rest.push(charge);
            }
        }
        let (first, last) = (taken[0].day, taken[taken.len() - 1].day);
        let crowd = rest
            .iter()
            .filter(|c| c.day >= first && c.day <= last)
            .count();
        chains.push(Chain {
            cadence,
            charges: taken,
            single,
            double,
            crowd,
        });
        charges = rest;
    }
    // What is left may be the first charge at a new price, for `stitch`.
    chains.extend(charges.into_iter().map(|charge| Chain {
        cadence: Cadence::Monthly,
        charges: vec![charge],
        single: 0,
        double: 0,
        crowd: 0,
    }));
    chains
}

/// Longest path through charges (sorted by day) where each step is one or
/// two cycles: (cadence, indices, one-cycle gaps, two-cycle gaps).
fn longest(charges: &[Charge], cadence: Cadence) -> Option<(Cadence, Vec<usize>, u32, u32)> {
    // best[i] = (length, single gaps, double gaps, previous index)
    let mut best: Vec<(usize, u32, u32, Option<usize>)> = vec![(1, 0, 0, None); charges.len()];
    for i in 0..charges.len() {
        for j in 0..i {
            let gap = (charges[i].day - charges[j].day).num_days();
            let Some(k) = cadence.cycles(gap) else {
                continue;
            };
            let (len, single, double, _) = best[j];
            let candidate = (
                len + 1,
                single + u32::from(k == 1),
                double + u32::from(k == 2),
                Some(j),
            );
            let current = best[i];
            if (candidate.0, candidate.1, std::cmp::Reverse(candidate.2))
                > (current.0, current.1, std::cmp::Reverse(current.2))
            {
                best[i] = candidate;
            }
        }
    }
    // Ties go to the run that ends latest.
    let end = (0..charges.len()).max_by_key(|&i| (best[i].0, best[i].1, i))?;
    let (_, single, double, _) = best[end];
    let mut path = vec![end];
    let mut at = end;
    while let Some(previous) = best[at].3 {
        path.push(previous);
        at = previous;
    }
    path.reverse();
    Some((cadence, path, single, double))
}

/// Joins runs at the same cadence where one picks up a cycle or two after
/// the other stops: a price change that moved the amount to a new band.
///
/// A lone charge (no cadence of its own) joins a run only at a modest
/// price step, so a one-off order a month after the last charge is not
/// read as a price rise.
fn stitch(chains: Vec<Chain>) -> Vec<Chain> {
    let (mut runs, lone): (Vec<Chain>, Vec<Chain>) = chains
        .into_iter()
        .partition(|chain| chain.charges.len() > 1);
    runs.sort_by(|a, b| {
        a.cadence
            .cmp(&b.cadence)
            .then_with(|| a.charges[0].day.cmp(&b.charges[0].day))
            .then_with(|| a.charges[0].record_ids.cmp(&b.charges[0].record_ids))
    });
    let mut out: Vec<Chain> = Vec::new();
    for chain in runs {
        let joined = out
            .iter_mut()
            .rev()
            .find(|prior| prior.cadence == chain.cadence && follows(prior, &chain, chain.cadence));
        match joined {
            Some(prior) => {
                let last = prior.charges[prior.charges.len() - 1].day;
                let gap = (chain.charges[0].day - last).num_days();
                match chain.cadence.cycles(gap) {
                    Some(1) => prior.single += 1,
                    _ => prior.double += 1,
                }
                prior.single += chain.single;
                prior.double += chain.double;
                prior.crowd += chain.crowd;
                prior.charges.extend(chain.charges);
            }
            None => out.push(chain),
        }
    }
    // Lone charges in day order, each onto the run it continues.
    let mut lone = lone;
    lone.sort_by(|a, b| {
        a.charges[0]
            .day
            .cmp(&b.charges[0].day)
            .then_with(|| a.charges[0].record_ids.cmp(&b.charges[0].record_ids))
    });
    for chain in lone {
        let charge = &chain.charges[0];
        let joined = out.iter_mut().find(|run| {
            follows(run, &chain, run.cadence)
                && modest_step(
                    run.charges[run.charges.len() - 1].amount_minor,
                    charge.amount_minor,
                )
        });
        if let Some(run) = joined {
            let last = run.charges[run.charges.len() - 1].day;
            match run.cadence.cycles((charge.day - last).num_days()) {
                Some(1) => run.single += 1,
                _ => run.double += 1,
            }
            run.charges.extend(chain.charges);
        }
    }
    out
}

/// `next` starts a cycle or two after `prior` ends.
fn follows(prior: &Chain, next: &Chain, cadence: Cadence) -> bool {
    let last = prior.charges[prior.charges.len() - 1].day;
    let first = next.charges[0].day;
    first > last && cadence.cycles((first - last).num_days()).is_some()
}

/// A price rise or cut a subscription plausibly makes: within 60%.
fn modest_step(from: Option<i64>, to: Option<i64>) -> bool {
    match (from, to) {
        (Some(from), Some(to)) if from > 0 && to > 0 => {
            let ratio = from.max(to) as f64 / from.min(to) as f64;
            ratio <= 1.6
        }
        _ => false,
    }
}

fn valid(chain: &Chain, stated: bool) -> bool {
    let enough = match chain.cadence {
        Cadence::Yearly => chain.charges.len() >= 2 && chain.single >= 1,
        _ => chain.charges.len() >= 3 && chain.single >= 2,
    };
    // One identical plan running alongside is allowed; more is a habit.
    let sparse = chain.crowd <= chain.charges.len();
    enough
        && sparse
        && chain.double <= chain.single
        && (stated || steady(&chain.charges))
}

/// Every change of amount sticks (the next charge keeps the new price),
/// and there are few of them.
fn steady(charges: &[Charge]) -> bool {
    let amounts: Vec<Option<i64>> = charges.iter().map(|c| c.amount_minor).collect();
    let mut changes = 0;
    for i in 1..amounts.len() {
        if same_price(amounts[i - 1], amounts[i]) {
            continue;
        }
        changes += 1;
        if let Some(next) = amounts.get(i + 1) {
            if !same_price(amounts[i], *next) {
                return false;
            }
        }
    }
    changes <= (charges.len() - 1) / 3 + 1
}

fn build(
    issuer_key: &str,
    issuer: &str,
    product_key: &str,
    product: Option<String>,
    stated: bool,
    currency: &str,
    chain: Chain,
) -> Subscription {
    let first = chain.charges[0].record_ids[0].clone();
    let key = format!("subscription|{issuer_key}|{product_key}|{currency}|{first}");
    let price_changes = chain
        .charges
        .windows(2)
        .filter_map(|pair| {
            let (from, to) = (pair[0].amount_minor?, pair[1].amount_minor?);
            (!same_price(Some(from), Some(to))).then(|| PriceChange {
                day: pair[1].day,
                record_id: pair[1].record_ids[0].clone(),
                from_minor: from,
                to_minor: to,
            })
        })
        .collect();
    let latest = chain
        .charges
        .iter()
        .rev()
        .find(|c| c.amount_minor.is_some());
    Subscription {
        id: format!("sub_{:016x}", fnv1a(&key)),
        key,
        issuer_key: issuer_key.to_string(),
        issuer: issuer.to_string(),
        product_key: product_key.to_string(),
        product: (!product_key.is_empty()).then_some(product).flatten(),
        product_stated: stated,
        cadence: chain.cadence,
        currency: (!currency.is_empty()).then(|| currency.to_string()),
        amount_minor: latest.and_then(|c| c.amount_minor),
        amount_checked: latest.is_some_and(|c| c.checked),
        single_gaps: chain.single,
        double_gaps: chain.double,
        charges: chain.charges,
        price_changes,
        status: Status::Active,
        status_reason: String::new(),
        next_expected: None,
        cancellation: None,
    }
}

/// A short stable id from the key, with no table to keep it in. FNV-1a is
/// enough: ids only need to differ between one mailbox's subscriptions.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A cancellation email after a subscription's last charge ends it. With
/// several subscriptions from the issuer, only the one the subject names.
fn apply_cancellations<Tz: TimeZone>(
    found: &mut [Subscription],
    issuer_key: &str,
    cancellations: &[CancellationInput],
    tz: &Tz,
) {
    let mut mine: Vec<&CancellationInput> = cancellations
        .iter()
        .filter(|c| c.issuer_key == issuer_key && is_cancellation(&c.subject))
        .collect();
    mine.sort_by(|a, b| {
        a.at.cmp(&b.at)
            .then_with(|| a.message_id.cmp(&b.message_id))
    });
    let several = found.len() > 1;
    for cancellation in mine {
        let day = local_day(cancellation.at, tz);
        let subject = cancellation.subject.to_lowercase();
        for subscription in found.iter_mut() {
            if day < subscription.last_charge().day {
                continue;
            }
            let named = subscription
                .product
                .as_deref()
                .is_some_and(|p| subject.contains(&p.to_lowercase()));
            if several && !named {
                continue;
            }
            subscription.cancellation = Some(Cancellation {
                day,
                message_id: cancellation.message_id.clone(),
            });
        }
    }
}

fn day_label(day: NaiveDate) -> String {
    day.format("%-d %b %Y").to_string()
}

fn set_status(subscription: &mut Subscription, today: NaiveDate) {
    let cadence = subscription.cadence;
    let last = subscription.last_charge().day;
    if let Some(cancellation) = &subscription.cancellation {
        subscription.status = Status::Ended;
        subscription.status_reason =
            format!("Cancellation email on {}", day_label(cancellation.day));
        return;
    }
    let grace = Duration::days(cadence.grace_days());
    let missed = (1..=2)
        .take_while(|n| cadence.add(last, *n) + grace < today)
        .count();
    let next = cadence.add(last, 1);
    match missed {
        0 => {
            subscription.status = Status::Active;
            subscription.next_expected = Some(next);
            subscription.status_reason = format!(
                "{} charges about {} apart since {}",
                subscription.charges.len(),
                cadence.interval(),
                subscription.start().format("%b %Y")
            );
        }
        1 => {
            subscription.status = Status::Overdue;
            subscription.next_expected = Some(next);
            subscription.status_reason = format!(
                "Expected around {}; no charge since {}",
                day_label(next),
                day_label(last)
            );
        }
        _ => {
            subscription.status = Status::Ended;
            subscription.status_reason = format!(
                "No charge since {}; 2 expected charges missed",
                day_label(last)
            );
        }
    }
}

/// How long before a yearly subscription's next charge it is worth a
/// look: the lead time a renewal email would typically give.
const RENEWAL_LEAD_DAYS: i64 = 14;

/// Something about a subscription worth a look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// The newest charge is at a new price.
    PriceChange {
        subscription_id: String,
        change: PriceChange,
    },
    /// The expected charge didn't come within the grace period.
    MissedCharge {
        subscription_id: String,
        expected: NaiveDate,
    },
    /// A yearly subscription's next charge is within the lead time: worth
    /// comparing quotes before it renews. A suggestion only; nothing
    /// files a to-do from it.
    RenewalApproaching {
        subscription_id: String,
        due: NaiveDate,
    },
}

/// Signals for live subscriptions: a price change while the newest charge
/// carries it, a missed charge while it is overdue, a yearly renewal
/// within its lead time.
pub fn signals(subscriptions: &[Subscription], today: NaiveDate) -> Vec<Signal> {
    let mut out = Vec::new();
    for subscription in subscriptions {
        if subscription.status == Status::Ended {
            continue;
        }
        if let Some(change) = subscription.price_changes.last() {
            if subscription
                .last_charge()
                .record_ids
                .contains(&change.record_id)
            {
                out.push(Signal::PriceChange {
                    subscription_id: subscription.id.clone(),
                    change: change.clone(),
                });
            }
        }
        if let (Status::Overdue, Some(expected)) = (subscription.status, subscription.next_expected)
        {
            out.push(Signal::MissedCharge {
                subscription_id: subscription.id.clone(),
                expected,
            });
        }
        if let (Status::Active, Cadence::Yearly, Some(due)) =
            (subscription.status, subscription.cadence, subscription.next_expected)
        {
            if (due - today).num_days() <= RENEWAL_LEAD_DAYS {
                out.push(Signal::RenewalApproaching {
                    subscription_id: subscription.id.clone(),
                    due,
                });
            }
        }
    }
    out
}

/// What live subscriptions cost, per currency, never converted:
/// (currency, per month, per year). The month is the year over twelve.
pub fn totals(subscriptions: &[Subscription]) -> Vec<(String, i64, i64)> {
    let mut per_year: BTreeMap<String, i64> = BTreeMap::new();
    for subscription in subscriptions {
        if subscription.status == Status::Ended {
            continue;
        }
        if let (Some(currency), Some(yearly)) =
            (&subscription.currency, subscription.yearly_cost_minor())
        {
            *per_year.entry(currency.clone()).or_default() += yearly;
        }
    }
    let mut out: Vec<(String, i64, i64)> = per_year
        .into_iter()
        .map(|(currency, year)| {
            let month = (year as f64 / 12.0).round() as i64;
            (currency, month, year)
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests;
