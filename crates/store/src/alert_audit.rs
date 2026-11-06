//! Read-only inputs for the alert audit. Provenance comes from the arrival
//! table, never from replaying authentication strings in message bodies.
use crate::StoredArrival;
use sqlx::{Row, SqliteConnection};

#[derive(Debug, Clone)]
pub struct AuditCandidate {
    pub account_id: String,
    pub sync_provider: Option<String>,
    pub has_list_id: bool,
    pub fact_json: String,
    pub arrival: StoredArrival,
    pub in_inbox: bool,
    pub unread: bool,
}

#[derive(Debug, Clone)]
pub struct AuditMail {
    pub account_id: String,
    pub from_host: String,
    pub received_at: Option<i64>,
}

pub async fn alert_audit_candidates(
    conn: &mut SqliteConnection,
) -> Result<Vec<AuditCandidate>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT f.account_id, ac.sync_provider, f.fact_json,
                (m.list_id IS NOT NULL OR json_extract(b.metadata_json, '$.list_id') IS NOT NULL) AS has_list_id,
                m.flags, a.received_at, a.verified_host, a.unverified_reason, COALESCE(a.is_live, 0) AS is_live,
                EXISTS (SELECT 1 FROM message_labels ml JOIN labels l ON l.id=ml.label_id
                        WHERE ml.message_id=m.id AND l.role='inbox') AS in_inbox
         FROM update_facts f JOIN messages m ON m.id=f.message_id JOIN accounts ac ON ac.id=m.account_id
         LEFT JOIN bodies b ON b.message_id=m.id LEFT JOIN message_arrivals a ON a.message_id=m.id
         WHERE json_extract(f.fact_json, '$.needs_you') IS NOT NULL AND m.direction='inbound'
         ORDER BY a.received_at, m.id"
    ).fetch_all(conn).await?;
    rows.iter()
        .map(|row| {
            let flags = mxr_core::MessageFlags::from_bits_truncate(
                u32::try_from(row.try_get::<i64, _>("flags")?).unwrap_or_default(),
            );
            Ok(AuditCandidate {
                account_id: row.try_get("account_id")?,
                sync_provider: row.try_get("sync_provider")?,
                fact_json: row.try_get("fact_json")?,
                has_list_id: row.try_get("has_list_id")?,
                in_inbox: row.try_get("in_inbox")?,
                unread: !flags.contains(mxr_core::MessageFlags::READ),
                arrival: StoredArrival {
                    received_at: crate::decode_optional_timestamp(row.try_get("received_at")?)?,
                    verified_host: row.try_get("verified_host")?,
                    is_live: row.try_get("is_live")?,
                    unverified_reason: row
                        .try_get::<Option<String>, _>("unverified_reason")?
                        .and_then(|code| mxr_core::UnverifiedReason::from_code(&code)),
                },
            })
        })
        .collect()
}

pub async fn alert_audit_inbound(
    conn: &mut SqliteConnection,
) -> Result<Vec<AuditMail>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT a.account_id, a.from_host, a.received_at FROM message_arrivals a
         JOIN messages m ON m.id=a.message_id WHERE m.direction='inbound'",
    )
    .fetch_all(conn)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(AuditMail {
                account_id: row.try_get("account_id")?,
                from_host: row.try_get("from_host")?,
                received_at: row.try_get("received_at")?,
            })
        })
        .collect()
}
