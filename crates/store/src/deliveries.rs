//! Deliveries: package/shipment tracking rows distilled from inbound mail.
//!
//! A delivery is one parcel (keyed by tracking number) or one not-yet-shipped
//! order. The daemon's post-sync fan-out runs a local heuristic (+ optional
//! LLM) over newly-upserted messages and upserts deliveries here, collapsing
//! the many emails of one shipment into a single row keyed by `dedup_key`.
//!
//! The store deliberately stays dumb: it persists/queries rows and links
//! provenance. Correlation, monotonic status advancement, and resolve/dismiss
//! *policy* live in the `deliveries` crate, which calls these primitives.
//!
//! Resolution is non-destructive: `resolved_at`/`delivered_at` move a row out
//! of the active list (auto on a "delivered" signal, or manual user resolve),
//! and `dismissed_at` hides a false positive — both keep the row and its
//! provenance for history and a "delivered" filter.

use crate::{
    decode_id, decode_json, decode_optional_timestamp, decode_timestamp, encode_json, trace_lookup,
    trace_query,
};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, DeliveryId, MessageId, ThreadId};
use serde::{Deserialize, Serialize};

/// One ordered/shipped item, schema.org- or LLM-derived. Stored as JSON in
/// `deliveries.items_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryItem {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<i64>,
}

/// A row from the `deliveries` table.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub id: DeliveryId,
    pub account_id: AccountId,
    /// Correlation key: tracking number when known, else "merchant|order".
    pub dedup_key: String,
    pub merchant: Option<String>,
    pub carrier: Option<String>,
    pub tracking_number: Option<String>,
    pub tracking_url: Option<String>,
    pub order_number: Option<String>,
    /// Normalized lifecycle status (text). Typed enum lives in the
    /// `deliveries` crate.
    pub status: String,
    pub eta_from: Option<DateTime<Utc>>,
    pub eta_until: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub items: Vec<DeliveryItem>,
    pub confidence: f64,
    /// How the row was detected: "schema" | "llm" | "heuristic".
    pub source: String,
    /// Latest contributing thread (for "open in mailbox").
    pub thread_id: Option<ThreadId>,
    pub last_event_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
}

/// Which slice of deliveries to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryListFilter {
    /// In-flight: not delivered, not dismissed. The default view.
    Active,
    /// Closed-out: delivered/resolved, not dismissed.
    Delivered,
    /// Everything except dismissed.
    All,
    /// Hidden false positives.
    Dismissed,
}

/// Maps an anonymous `query!` row (selecting the canonical column list) into a
/// `Delivery`. All select sites use the same projection so this stays one
/// place. `decode_*` helpers come from the crate root.
macro_rules! delivery_from_row {
    ($r:expr) => {{
        let r = $r;
        Ok::<Delivery, sqlx::Error>(Delivery {
            id: decode_id(&r.id)?,
            account_id: decode_id(&r.account_id)?,
            dedup_key: r.dedup_key,
            merchant: r.merchant,
            carrier: r.carrier,
            tracking_number: r.tracking_number,
            tracking_url: r.tracking_url,
            order_number: r.order_number,
            status: r.status,
            eta_from: decode_optional_timestamp(r.eta_from)?,
            eta_until: decode_optional_timestamp(r.eta_until)?,
            delivered_at: decode_optional_timestamp(r.delivered_at)?,
            items: decode_json(&r.items_json)?,
            confidence: r.confidence,
            source: r.source,
            thread_id: match r.thread_id {
                Some(s) => Some(decode_id(&s)?),
                None => None,
            },
            last_event_at: decode_timestamp(r.last_event_at)?,
            created_at: decode_timestamp(r.created_at)?,
            updated_at: decode_timestamp(r.updated_at)?,
            resolved_at: decode_optional_timestamp(r.resolved_at)?,
            dismissed_at: decode_optional_timestamp(r.dismissed_at)?,
        })
    }};
}

impl super::Store {
    /// Insert a brand-new delivery. Callers (the `deliveries` crate's
    /// lifecycle layer) check `get_delivery_by_dedup` first and route to
    /// `update_delivery` on a hit, so this does not upsert.
    pub async fn insert_delivery(&self, d: &Delivery) -> Result<(), sqlx::Error> {
        let id = d.id.as_str();
        let account_id = d.account_id.as_str();
        let items_json = encode_json(&d.items)?;
        let thread_id = d.thread_id.as_ref().map(|t| t.as_str());
        let eta_from = d.eta_from.map(|x| x.timestamp());
        let eta_until = d.eta_until.map(|x| x.timestamp());
        let delivered_at = d.delivered_at.map(|x| x.timestamp());
        let resolved_at = d.resolved_at.map(|x| x.timestamp());
        let dismissed_at = d.dismissed_at.map(|x| x.timestamp());
        let last_event_at = d.last_event_at.timestamp();
        let created_at = d.created_at.timestamp();
        let updated_at = d.updated_at.timestamp();
        sqlx::query!(
            r#"INSERT INTO deliveries
                   (id, account_id, dedup_key, merchant, carrier,
                    tracking_number, tracking_url, order_number, status,
                    eta_from, eta_until, delivered_at, items_json, confidence,
                    source, thread_id, last_event_at, created_at, updated_at,
                    resolved_at, dismissed_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            id,
            account_id,
            d.dedup_key,
            d.merchant,
            d.carrier,
            d.tracking_number,
            d.tracking_url,
            d.order_number,
            d.status,
            eta_from,
            eta_until,
            delivered_at,
            items_json,
            d.confidence,
            d.source,
            thread_id,
            last_event_at,
            created_at,
            updated_at,
            resolved_at,
            dismissed_at,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Overwrite the mutable fields of an existing delivery (by `id`). Used by
    /// the lifecycle layer after merging a new email into an existing row;
    /// `id` and `created_at` are preserved. `dedup_key` may change here when an
    /// order-only row later gains a tracking number.
    pub async fn update_delivery(&self, d: &Delivery) -> Result<(), sqlx::Error> {
        let id = d.id.as_str();
        let items_json = encode_json(&d.items)?;
        let thread_id = d.thread_id.as_ref().map(|t| t.as_str());
        let eta_from = d.eta_from.map(|x| x.timestamp());
        let eta_until = d.eta_until.map(|x| x.timestamp());
        let delivered_at = d.delivered_at.map(|x| x.timestamp());
        let resolved_at = d.resolved_at.map(|x| x.timestamp());
        let dismissed_at = d.dismissed_at.map(|x| x.timestamp());
        let last_event_at = d.last_event_at.timestamp();
        let updated_at = d.updated_at.timestamp();
        sqlx::query!(
            r#"UPDATE deliveries SET
                   dedup_key = ?, merchant = ?, carrier = ?,
                   tracking_number = ?, tracking_url = ?, order_number = ?,
                   status = ?, eta_from = ?, eta_until = ?, delivered_at = ?,
                   items_json = ?, confidence = ?, source = ?, thread_id = ?,
                   last_event_at = ?, updated_at = ?, resolved_at = ?,
                   dismissed_at = ?
               WHERE id = ?"#,
            d.dedup_key,
            d.merchant,
            d.carrier,
            d.tracking_number,
            d.tracking_url,
            d.order_number,
            d.status,
            eta_from,
            eta_until,
            delivered_at,
            items_json,
            d.confidence,
            d.source,
            thread_id,
            last_event_at,
            updated_at,
            resolved_at,
            dismissed_at,
            id,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Fetch a delivery by primary key.
    pub async fn get_delivery(&self, id: &DeliveryId) -> Result<Option<Delivery>, sqlx::Error> {
        let id = id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!",
                      dedup_key as "dedup_key!", merchant, carrier,
                      tracking_number, tracking_url, order_number,
                      status as "status!", eta_from, eta_until, delivered_at,
                      items_json as "items_json!", confidence as "confidence!: f64",
                      source as "source!", thread_id,
                      last_event_at as "last_event_at!",
                      created_at as "created_at!", updated_at as "updated_at!",
                      resolved_at, dismissed_at
               FROM deliveries WHERE id = ?"#,
            id,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("deliveries.get", started_at, row.is_some());
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(delivery_from_row!(r)?)),
        }
    }

    /// Fetch the delivery for a correlation key within an account, if any.
    /// The lifecycle layer uses this to decide insert-vs-merge.
    pub async fn get_delivery_by_dedup(
        &self,
        account_id: &AccountId,
        dedup_key: &str,
    ) -> Result<Option<Delivery>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!",
                      dedup_key as "dedup_key!", merchant, carrier,
                      tracking_number, tracking_url, order_number,
                      status as "status!", eta_from, eta_until, delivered_at,
                      items_json as "items_json!", confidence as "confidence!: f64",
                      source as "source!", thread_id,
                      last_event_at as "last_event_at!",
                      created_at as "created_at!", updated_at as "updated_at!",
                      resolved_at, dismissed_at
               FROM deliveries WHERE account_id = ? AND dedup_key = ?"#,
            aid,
            dedup_key,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("deliveries.get_by_dedup", started_at, row.is_some());
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(delivery_from_row!(r)?)),
        }
    }

    /// List deliveries across all accounts for the given filter, newest first.
    pub async fn list_deliveries(
        &self,
        filter: DeliveryListFilter,
    ) -> Result<Vec<Delivery>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        // Each arm uses the same projection; only the predicate/order differs,
        // which `query!` requires as distinct literals.
        let rows = match filter {
            DeliveryListFilter::Active => {
                sqlx::query!(
                    r#"SELECT id as "id!", account_id as "account_id!",
                              dedup_key as "dedup_key!", merchant, carrier,
                              tracking_number, tracking_url, order_number,
                              status as "status!", eta_from, eta_until, delivered_at,
                              items_json as "items_json!", confidence as "confidence!: f64",
                              source as "source!", thread_id,
                              last_event_at as "last_event_at!",
                              created_at as "created_at!", updated_at as "updated_at!",
                              resolved_at, dismissed_at
                       FROM deliveries
                       WHERE dismissed_at IS NULL AND delivered_at IS NULL
                       ORDER BY last_event_at DESC"#,
                )
                .fetch_all(self.reader())
                .await?
                .into_iter()
                .map(|r| delivery_from_row!(r))
                .collect::<Result<Vec<_>, _>>()?
            }
            DeliveryListFilter::Delivered => {
                sqlx::query!(
                    r#"SELECT id as "id!", account_id as "account_id!",
                              dedup_key as "dedup_key!", merchant, carrier,
                              tracking_number, tracking_url, order_number,
                              status as "status!", eta_from, eta_until, delivered_at,
                              items_json as "items_json!", confidence as "confidence!: f64",
                              source as "source!", thread_id,
                              last_event_at as "last_event_at!",
                              created_at as "created_at!", updated_at as "updated_at!",
                              resolved_at, dismissed_at
                       FROM deliveries
                       WHERE dismissed_at IS NULL AND delivered_at IS NOT NULL
                       ORDER BY delivered_at DESC"#,
                )
                .fetch_all(self.reader())
                .await?
                .into_iter()
                .map(|r| delivery_from_row!(r))
                .collect::<Result<Vec<_>, _>>()?
            }
            DeliveryListFilter::All => {
                sqlx::query!(
                    r#"SELECT id as "id!", account_id as "account_id!",
                              dedup_key as "dedup_key!", merchant, carrier,
                              tracking_number, tracking_url, order_number,
                              status as "status!", eta_from, eta_until, delivered_at,
                              items_json as "items_json!", confidence as "confidence!: f64",
                              source as "source!", thread_id,
                              last_event_at as "last_event_at!",
                              created_at as "created_at!", updated_at as "updated_at!",
                              resolved_at, dismissed_at
                       FROM deliveries
                       WHERE dismissed_at IS NULL
                       ORDER BY last_event_at DESC"#,
                )
                .fetch_all(self.reader())
                .await?
                .into_iter()
                .map(|r| delivery_from_row!(r))
                .collect::<Result<Vec<_>, _>>()?
            }
            DeliveryListFilter::Dismissed => {
                sqlx::query!(
                    r#"SELECT id as "id!", account_id as "account_id!",
                              dedup_key as "dedup_key!", merchant, carrier,
                              tracking_number, tracking_url, order_number,
                              status as "status!", eta_from, eta_until, delivered_at,
                              items_json as "items_json!", confidence as "confidence!: f64",
                              source as "source!", thread_id,
                              last_event_at as "last_event_at!",
                              created_at as "created_at!", updated_at as "updated_at!",
                              resolved_at, dismissed_at
                       FROM deliveries
                       WHERE dismissed_at IS NOT NULL
                       ORDER BY dismissed_at DESC"#,
                )
                .fetch_all(self.reader())
                .await?
                .into_iter()
                .map(|r| delivery_from_row!(r))
                .collect::<Result<Vec<_>, _>>()?
            }
        };
        trace_query("deliveries.list", started_at, rows.len());
        Ok(rows)
    }

    /// Resolve a delivery: mark it delivered/closed so it leaves the active
    /// list. Used both by auto "delivered" detection and the manual
    /// user-resolve action. Idempotent; preserves an existing `delivered_at`.
    pub async fn resolve_delivery(
        &self,
        id: &DeliveryId,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let id = id.as_str();
        let ts = now.timestamp();
        sqlx::query!(
            r#"UPDATE deliveries SET
                   status = 'delivered',
                   delivered_at = COALESCE(delivered_at, ?),
                   resolved_at = COALESCE(resolved_at, ?),
                   updated_at = ?
               WHERE id = ?"#,
            ts,
            ts,
            ts,
            id,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Dismiss a delivery (hide a false positive). Non-destructive.
    pub async fn dismiss_delivery(
        &self,
        id: &DeliveryId,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let id = id.as_str();
        let ts = now.timestamp();
        sqlx::query!(
            r#"UPDATE deliveries SET dismissed_at = ?, updated_at = ? WHERE id = ?"#,
            ts,
            ts,
            id,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Record that a message contributed to a delivery (provenance). Idempotent
    /// per (delivery, message); refreshes the signalled stage on repeat.
    pub async fn link_delivery_message(
        &self,
        delivery_id: &DeliveryId,
        message_id: &MessageId,
        thread_id: Option<&ThreadId>,
        email_kind: Option<&str>,
        detected_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let did = delivery_id.as_str();
        let mid = message_id.as_str();
        let tid = thread_id.map(|t| t.as_str());
        let ts = detected_at.timestamp();
        sqlx::query!(
            r#"INSERT INTO delivery_messages
                   (delivery_id, message_id, thread_id, email_kind, detected_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(delivery_id, message_id) DO UPDATE SET
                   thread_id = excluded.thread_id,
                   email_kind = excluded.email_kind,
                   detected_at = excluded.detected_at"#,
            did,
            mid,
            tid,
            email_kind,
            ts,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Source message IDs linked to a delivery, oldest first.
    pub async fn delivery_message_ids(
        &self,
        delivery_id: &DeliveryId,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let did = delivery_id.as_str();
        let rows = sqlx::query!(
            r#"SELECT message_id as "message_id!"
               FROM delivery_messages
               WHERE delivery_id = ?
               ORDER BY detected_at ASC"#,
            did,
        )
        .fetch_all(self.reader())
        .await?;
        rows.into_iter().map(|r| decode_id(&r.message_id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
    use super::{Delivery, DeliveryItem, DeliveryListFilter};
    use chrono::{Duration, TimeZone, Utc};
    use mxr_core::id::{AccountId, DeliveryId, MessageId, ThreadId};
    use mxr_core::types::Envelope;

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    async fn seed(store: &Store) -> (AccountId, Envelope) {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        env.id = MessageId::new();
        env.provider_id = "fake-1".into();
        store.upsert_envelope(&env).await.unwrap();
        (account.id, env)
    }

    fn sample(account_id: &AccountId, dedup_key: &str, status: &str) -> Delivery {
        Delivery {
            id: DeliveryId::new(),
            account_id: account_id.clone(),
            dedup_key: dedup_key.to_string(),
            merchant: Some("Acme".into()),
            carrier: Some("ups".into()),
            tracking_number: Some("1Z999".into()),
            tracking_url: Some("https://example.test/track".into()),
            order_number: Some("A-100".into()),
            status: status.to_string(),
            eta_from: None,
            eta_until: Some(anchor() + Duration::days(2)),
            delivered_at: None,
            items: vec![DeliveryItem {
                name: "Widget".into(),
                quantity: Some(2),
            }],
            confidence: 0.9,
            source: "heuristic".into(),
            thread_id: None,
            last_event_at: anchor(),
            created_at: anchor(),
            updated_at: anchor(),
            resolved_at: None,
            dismissed_at: None,
        }
    }

    #[tokio::test]
    async fn insert_and_round_trip() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, _) = seed(&store).await;
        let d = sample(&account_id, "1Z999", "in_transit");
        store.insert_delivery(&d).await.unwrap();

        let got = store.get_delivery(&d.id).await.unwrap().expect("stored");
        assert_eq!(got, d);

        let by_dedup = store
            .get_delivery_by_dedup(&account_id, "1Z999")
            .await
            .unwrap()
            .expect("dedup hit");
        assert_eq!(by_dedup.id, d.id);
    }

    #[tokio::test]
    async fn update_changes_fields_and_dedup_key() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, _) = seed(&store).await;
        let mut d = sample(&account_id, "Acme|A-100", "ordered");
        d.tracking_number = None;
        store.insert_delivery(&d).await.unwrap();

        // Order-only row later gains a tracking number → dedup_key migrates.
        d.tracking_number = Some("1Z42".into());
        d.dedup_key = "1Z42".into();
        d.status = "shipped".into();
        store.update_delivery(&d).await.unwrap();

        assert!(store
            .get_delivery_by_dedup(&account_id, "Acme|A-100")
            .await
            .unwrap()
            .is_none());
        let got = store
            .get_delivery_by_dedup(&account_id, "1Z42")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.status, "shipped");
        assert_eq!(got.id, d.id, "same row, migrated key");
    }

    #[tokio::test]
    async fn list_filters_partition_rows() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, _) = seed(&store).await;

        let active = sample(&account_id, "k-active", "in_transit");
        store.insert_delivery(&active).await.unwrap();

        let mut delivered = sample(&account_id, "k-delivered", "delivered");
        delivered.delivered_at = Some(anchor());
        delivered.resolved_at = Some(anchor());
        store.insert_delivery(&delivered).await.unwrap();

        let mut dismissed = sample(&account_id, "k-dismissed", "in_transit");
        dismissed.dismissed_at = Some(anchor());
        store.insert_delivery(&dismissed).await.unwrap();

        let ids = |v: Vec<Delivery>| v.into_iter().map(|d| d.id).collect::<Vec<_>>();
        assert_eq!(
            ids(store.list_deliveries(DeliveryListFilter::Active).await.unwrap()),
            vec![active.id.clone()]
        );
        assert_eq!(
            ids(store
                .list_deliveries(DeliveryListFilter::Delivered)
                .await
                .unwrap()),
            vec![delivered.id.clone()]
        );
        assert_eq!(
            ids(store
                .list_deliveries(DeliveryListFilter::Dismissed)
                .await
                .unwrap()),
            vec![dismissed.id.clone()]
        );
        // All excludes only the dismissed row.
        let all = ids(store.list_deliveries(DeliveryListFilter::All).await.unwrap());
        assert_eq!(all.len(), 2);
        assert!(all.contains(&active.id) && all.contains(&delivered.id));
    }

    #[tokio::test]
    async fn resolve_marks_delivered_and_leaves_active() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, _) = seed(&store).await;
        let d = sample(&account_id, "k1", "out_for_delivery");
        store.insert_delivery(&d).await.unwrap();

        store.resolve_delivery(&d.id, anchor()).await.unwrap();
        let got = store.get_delivery(&d.id).await.unwrap().unwrap();
        assert_eq!(got.status, "delivered");
        assert_eq!(got.delivered_at, Some(anchor()));
        assert_eq!(got.resolved_at, Some(anchor()));
        assert!(store
            .list_deliveries(DeliveryListFilter::Active)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn dismiss_hides_from_active_and_all() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, _) = seed(&store).await;
        let d = sample(&account_id, "k1", "in_transit");
        store.insert_delivery(&d).await.unwrap();
        store.dismiss_delivery(&d.id, anchor()).await.unwrap();
        assert!(store
            .list_deliveries(DeliveryListFilter::Active)
            .await
            .unwrap()
            .is_empty());
        assert!(store
            .list_deliveries(DeliveryListFilter::All)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn provenance_links_are_idempotent_and_ordered() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mut envs = Vec::new();
        for i in 0..3 {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            env.id = MessageId::new();
            env.provider_id = format!("m-{i}");
            store.upsert_envelope(&env).await.unwrap();
            envs.push(env);
        }
        let d = sample(&account.id, "k1", "ordered");
        store.insert_delivery(&d).await.unwrap();

        let tid = ThreadId::new();
        for (i, env) in envs.iter().enumerate() {
            store
                .link_delivery_message(
                    &d.id,
                    &env.id,
                    Some(&tid),
                    Some("shipped"),
                    anchor() + Duration::minutes(i as i64),
                )
                .await
                .unwrap();
        }
        // Re-link the first one with a new stage; still one row per message.
        store
            .link_delivery_message(&d.id, &envs[0].id, Some(&tid), Some("delivered"), anchor())
            .await
            .unwrap();

        let linked = store.delivery_message_ids(&d.id).await.unwrap();
        assert_eq!(linked.len(), 3);
        assert!(linked.contains(&envs[0].id));
    }
}
