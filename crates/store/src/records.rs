//! Archive records: one row per receipt, order, booking, invoice,
//! statement, ticket, contract, warranty or account, built from one or more
//! emails.
//!
//! The store keeps rows, field candidates and links; what a record means
//! (kinds, detection, grouping, the answer box) lives in the `mxr-records`
//! crate, which calls these primitives, as To do does. Queries are
//! unchecked (`sqlx::query`) so the tables need no offline cache entry.
//!
//! Every field value is a row in `record_fields` with its source, and the
//! `records` columns hold the winner, recomputed by [`RECOMPUTE_SQL`]
//! whenever a candidate changes or a source email is deleted.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, in_list};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

/// Fields whose value is money or a date. A record is checked only when
/// the winning candidate of each of these is checked.
pub const RECORD_CHECKED_FIELDS: &[&str] = &[
    "amount",
    "issued_at",
    "span_start",
    "span_end",
    "delivered_at",
    "return_by",
    "warranty_until",
    "valid_until",
];

/// The winning candidate of `field` for `records.id`: highest rank, then
/// newest, then the larger source key so ties never flip.
macro_rules! winner {
    ($column:literal, $field:literal) => {
        concat!(
            "(SELECT f.",
            $column,
            " FROM record_fields f WHERE f.record_id = records.id AND f.field = '",
            $field,
            "' ORDER BY f.rank DESC, f.observed_at DESC, f.source_key DESC LIMIT 1)"
        )
    };
}

/// The SET list that recomputes every winning column of a record from its
/// field candidates and source emails. Shared by the filing path and the
/// message delete, so both leave the same columns.
macro_rules! recompute_set {
    () => {
        concat!(
    "UPDATE records SET ",
    "kind = COALESCE(",
    winner!("value_text", "kind"),
    ", kind), ",
    "issuer = ",
    winner!("value_text", "issuer"),
    ", ",
    "issuer_key = LOWER(TRIM(",
    winner!("value_text", "issuer"),
    ")), ",
    "title = ",
    winner!("value_text", "title"),
    ", ",
    "reference = ",
    winner!("value_text", "reference"),
    ", ",
    "amount_minor = ",
    winner!("value_int", "amount"),
    ", ",
    "currency = ",
    winner!("value_text", "amount"),
    ", ",
    "issued_at = ",
    winner!("value_int", "issued_at"),
    ", ",
    "span_start = ",
    winner!("value_int", "span_start"),
    ", ",
    "span_end = ",
    winner!("value_int", "span_end"),
    ", ",
    "place = ",
    winner!("value_text", "place"),
    ", ",
    "delivered_at = ",
    winner!("value_int", "delivered_at"),
    ", ",
    "return_by = ",
    winner!("value_int", "return_by"),
    ", ",
    "warranty_until = ",
    winner!("value_int", "warranty_until"),
    ", ",
    "valid_until = ",
    winner!("value_int", "valid_until"),
    ", ",
    "checked = NOT EXISTS (
        SELECT 1 FROM record_fields w
        WHERE w.record_id = records.id
          AND w.field IN ('amount', 'issued_at', 'span_start', 'span_end', 'delivered_at',
                          'return_by', 'warranty_until', 'valid_until')
          AND w.checked = 0
          AND NOT EXISTS (
              SELECT 1 FROM record_fields b
              WHERE b.record_id = w.record_id AND b.field = w.field
                AND (b.rank > w.rank
                     OR (b.rank = w.rank AND (b.observed_at > w.observed_at
                         OR (b.observed_at = w.observed_at AND b.source_key > w.source_key)))))), ",
    "thread_id = COALESCE((SELECT rm.thread_id FROM record_messages rm WHERE rm.record_id = records.id
                  ORDER BY rm.message_at DESC, rm.message_id DESC LIMIT 1), thread_id), ",
    "last_message_at = COALESCE((SELECT MAX(rm.message_at) FROM record_messages rm
                        WHERE rm.record_id = records.id), last_message_at) ",
        )
    };
}

/// Recomputes the records listed in `temp.mxr_records`.
pub(crate) const RECOMPUTE_SQL: &str = concat!(
    recompute_set!(),
    "WHERE id IN (SELECT id FROM temp.mxr_records)"
);

/// Recomputes one record, bound at `?1`.
const RECOMPUTE_ONE_SQL: &str = concat!(recompute_set!(), "WHERE id = ?1");

/// A row of `records`. Kinds, origins and stages are plain strings here;
/// their vocabulary lives in `mxr-records`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveRecord {
    pub id: String,
    pub account_id: AccountId,
    pub dedup_key: String,
    pub kind: String,
    pub issuer: Option<String>,
    pub issuer_key: Option<String>,
    pub title: Option<String>,
    pub reference: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub issued_at: Option<DateTime<Utc>>,
    pub span_start: Option<DateTime<Utc>>,
    pub span_end: Option<DateTime<Utc>>,
    pub place: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub return_by: Option<DateTime<Utc>>,
    pub warranty_until: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub checked: bool,
    pub group_id: Option<String>,
    pub origin: String,
    pub reason: String,
    pub thread_id: Option<ThreadId>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub rules_version: i64,
    pub dismissed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ArchiveRecord {
    /// The date the ledger files it under: the transaction's, else the
    /// newest email's.
    pub fn ledger_date(&self) -> Option<DateTime<Utc>> {
        self.issued_at.or(self.last_message_at)
    }
}

/// One candidate value for one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFieldValue {
    pub field: String,
    /// The message id, `user`, or `todo:<id>`.
    pub source_key: String,
    pub message_id: Option<MessageId>,
    /// schema | rule | delivery | todo | user
    pub source: String,
    pub rank: i64,
    pub value_text: Option<String>,
    pub value_int: Option<i64>,
    pub checked: bool,
    pub evidence: Option<String>,
    pub observed_at: DateTime<Utc>,
}

/// A source email of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSource {
    pub record_id: String,
    pub message_id: MessageId,
    pub thread_id: Option<ThreadId>,
    pub stage: String,
    pub message_at: DateTime<Utc>,
    pub filed_by: String,
    pub subject: String,
}

/// An attachment of a record's source emails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDocument {
    pub record_id: String,
    pub message_id: MessageId,
    pub attachment_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub local_path: Option<String>,
}

impl RecordDocument {
    pub fn is_pdf(&self) -> bool {
        self.mime_type.eq_ignore_ascii_case("application/pdf")
            || self.filename.to_ascii_lowercase().ends_with(".pdf")
    }
}

/// A record group: a trip or a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordGroup {
    pub id: String,
    pub account_id: AccountId,
    pub kind: String,
    pub group_key: String,
    pub title: String,
    pub span_start: Option<DateTime<Utc>>,
    pub span_end: Option<DateTime<Utc>>,
}

/// The email a filing links, and what it said about the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLink {
    pub message_id: MessageId,
    pub thread_id: Option<ThreadId>,
    pub stage: String,
    pub message_at: DateTime<Utc>,
    /// detector | delivery | todo | manual | sender
    pub filed_by: String,
}

/// What a detector, a delivery, a to-do or the user files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFiling {
    pub id: String,
    pub account_id: AccountId,
    pub dedup_key: String,
    pub kind: String,
    pub origin: String,
    pub reason: String,
    pub rules_version: i64,
    pub links: Vec<RecordLink>,
    pub fields: Vec<RecordFieldValue>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordFiled {
    Inserted {
        id: String,
    },
    Updated {
        id: String,
    },
    /// Already filed with these links and values.
    Unchanged {
        id: String,
    },
    /// Dismissed as not a record earlier: the correction stands and the
    /// row is not touched.
    Dismissed {
        id: String,
    },
}

impl RecordFiled {
    pub fn id(&self) -> &str {
        match self {
            Self::Inserted { id }
            | Self::Updated { id }
            | Self::Unchanged { id }
            | Self::Dismissed { id } => id,
        }
    }
}

/// Which records a list returns. Every condition is optional; `None`
/// accounts means every account.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordQuery {
    pub account_ids: Option<Vec<AccountId>>,
    pub kinds: Vec<String>,
    pub issuer_key: Option<String>,
    /// Ledger dates in `[from, until)`.
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub min_amount_minor: Option<i64>,
    pub max_amount_minor: Option<i64>,
    pub has_pdf: Option<bool>,
    pub checked: Option<bool>,
    pub group_id: Option<String>,
    /// Only records with a date still ahead of this instant (a trip, a
    /// ticket, a return window, a warranty): the coming-up strip.
    pub moment_after: Option<DateTime<Utc>>,
    pub include_dismissed: bool,
}

/// The dedup key of a record a ticked-off to-do made itself.
pub fn todo_record_dedup_key(todo_id: &str) -> String {
    format!("todo|{todo_id}")
}

/// The source key of the field candidates a ticked-off to-do carried.
pub fn todo_record_source_key(todo_id: &str) -> String {
    format!("todo:{todo_id}")
}

/// A correction the user makes to one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordFieldEdit {
    /// The user's value: text in `value_text`; an amount as minor units in
    /// `value_int` with the currency in `value_text`; a date as unix
    /// seconds in `value_int`.
    Set {
        field: String,
        value_text: Option<String>,
        value_int: Option<i64>,
    },
    /// The winning value becomes the user's, checked.
    Confirm { field: String },
    /// Every money and date field whose winner is unchecked.
    ConfirmUnchecked,
    /// Drop the user's value.
    Clear { field: String },
}

/// A per-sender correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSenderRule {
    pub account_id: AccountId,
    pub sender_email: String,
    /// always | never, or `None` when only the issuer name is set.
    pub verdict: Option<String>,
    pub kind: Option<String>,
    pub issuer_name: Option<String>,
    pub decided_at: DateTime<Utc>,
}

/// Where an account's first pass over history has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRun {
    pub rules_version: i64,
    pub cursor: Option<(DateTime<Utc>, MessageId)>,
    pub scanned: i64,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A PDF of a record's email that is not on disk yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPdfToFetch {
    pub message_id: MessageId,
    pub attachment_id: String,
    pub size_bytes: i64,
}

const RECORD_COLUMNS: &str =
    "id, account_id, dedup_key, kind, issuer, issuer_key, title, reference,
    amount_minor, currency, issued_at, span_start, span_end, place, delivered_at, return_by,
    warranty_until, valid_until, checked, group_id, origin, reason, thread_id, last_message_at,
    rules_version, dismissed_at, created_at, updated_at";

fn row_to_record(row: &SqliteRow) -> Result<ArchiveRecord, sqlx::Error> {
    let ts = |name: &str| -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        decode_optional_timestamp(row.try_get(name)?)
    };
    Ok(ArchiveRecord {
        id: row.try_get("id")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        dedup_key: row.try_get("dedup_key")?,
        kind: row.try_get("kind")?,
        issuer: row.try_get("issuer")?,
        issuer_key: row.try_get("issuer_key")?,
        title: row.try_get("title")?,
        reference: row.try_get("reference")?,
        amount_minor: row.try_get("amount_minor")?,
        currency: row.try_get("currency")?,
        issued_at: ts("issued_at")?,
        span_start: ts("span_start")?,
        span_end: ts("span_end")?,
        place: row.try_get("place")?,
        delivered_at: ts("delivered_at")?,
        return_by: ts("return_by")?,
        warranty_until: ts("warranty_until")?,
        valid_until: ts("valid_until")?,
        checked: row.try_get::<i64, _>("checked")? != 0,
        group_id: row.try_get("group_id")?,
        origin: row.try_get("origin")?,
        reason: row.try_get("reason")?,
        thread_id: row
            .try_get::<Option<String>, _>("thread_id")?
            .map(|id| decode_id(&id))
            .transpose()?,
        last_message_at: ts("last_message_at")?,
        rules_version: row.try_get("rules_version")?,
        dismissed_at: ts("dismissed_at")?,
        created_at: decode_timestamp(row.try_get("created_at")?)?,
        updated_at: decode_timestamp(row.try_get("updated_at")?)?,
    })
}

fn row_to_field(row: &SqliteRow) -> Result<(String, RecordFieldValue), sqlx::Error> {
    Ok((
        row.try_get("record_id")?,
        RecordFieldValue {
            field: row.try_get("field")?,
            source_key: row.try_get("source_key")?,
            message_id: row
                .try_get::<Option<String>, _>("message_id")?
                .map(|id| decode_id(&id))
                .transpose()?,
            source: row.try_get("source")?,
            rank: row.try_get("rank")?,
            value_text: row.try_get("value_text")?,
            value_int: row.try_get("value_int")?,
            checked: row.try_get::<i64, _>("checked")? != 0,
            evidence: row.try_get("evidence")?,
            observed_at: decode_timestamp(row.try_get("observed_at")?)?,
        },
    ))
}

/// Binds a list of string ids into `sql`, which holds one `{}` for the
/// `IN (...)` list.
async fn fetch_by_ids(
    pool: &sqlx::SqlitePool,
    sql_template: &str,
    ids: &[String],
) -> Result<Vec<SqliteRow>, sqlx::Error> {
    let mut out = Vec::new();
    for chunk in ids.chunks(crate::SQLITE_BIND_CHUNK) {
        let sql = sql_template.replace("{}", &in_list(chunk.len()));
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            query = query.bind(id.as_str());
        }
        out.extend(query.fetch_all(pool).await?);
    }
    Ok(out)
}

async fn recompute_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record_ids: &[&str],
) -> Result<(), sqlx::Error> {
    if let [only] = record_ids {
        sqlx::query(RECOMPUTE_ONE_SQL)
            .bind(*only)
            .execute(&mut **tx)
            .await?;
        return Ok(());
    }
    sqlx::query("CREATE TEMP TABLE IF NOT EXISTS mxr_records (id TEXT PRIMARY KEY)")
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM temp.mxr_records")
        .execute(&mut **tx)
        .await?;
    for id in record_ids {
        sqlx::query("INSERT OR IGNORE INTO temp.mxr_records (id) VALUES (?)")
            .bind(*id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(RECOMPUTE_SQL).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM temp.mxr_records")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Writes a field candidate. With `only_if_changed`, an existing candidate
/// with the same value and checked state is left as it is (a re-scan of the
/// same email); returns whether a row was written.
async fn upsert_field_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record_id: &str,
    value: &RecordFieldValue,
    only_if_changed: bool,
) -> Result<bool, sqlx::Error> {
    const UPSERT: &str = "INSERT INTO record_fields
            (record_id, field, source_key, message_id, source, rank, value_text, value_int,
             checked, evidence, observed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(record_id, field, source_key) DO UPDATE SET
            message_id = excluded.message_id,
            source = excluded.source,
            rank = excluded.rank,
            value_text = excluded.value_text,
            value_int = excluded.value_int,
            checked = excluded.checked,
            evidence = excluded.evidence,
            observed_at = excluded.observed_at";
    let sql = if only_if_changed {
        sqlx::AssertSqlSafe(format!(
            "{UPSERT} WHERE record_fields.value_text IS NOT excluded.value_text
                OR record_fields.value_int IS NOT excluded.value_int
                OR record_fields.checked <> excluded.checked"
        ))
    } else {
        sqlx::AssertSqlSafe(UPSERT.to_string())
    };
    let written = sqlx::query(sql)
        .bind(record_id)
        .bind(&value.field)
        .bind(&value.source_key)
        .bind(value.message_id.as_ref().map(MessageId::as_str))
        .bind(&value.source)
        .bind(value.rank)
        .bind(&value.value_text)
        .bind(value.value_int)
        .bind(i64::from(value.checked))
        .bind(&value.evidence)
        .bind(value.observed_at.timestamp())
        .execute(&mut **tx)
        .await?
        .rows_affected();
    Ok(written > 0)
}

async fn file_record_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    filing: &RecordFiling,
) -> Result<RecordFiled, sqlx::Error> {
    let existing: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT id, dismissed_at FROM records WHERE account_id = ?1 AND dedup_key = ?2",
    )
    .bind(filing.account_id.as_str())
    .bind(&filing.dedup_key)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((id, Some(_))) = existing {
        return Ok(RecordFiled::Dismissed { id });
    }
    let (record_id, inserted) = match existing {
        Some((id, _)) => (id, false),
        None => {
            sqlx::query(
                "INSERT INTO records
                    (id, account_id, dedup_key, kind, origin, reason, rules_version,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            )
            .bind(&filing.id)
            .bind(filing.account_id.as_str())
            .bind(&filing.dedup_key)
            .bind(&filing.kind)
            .bind(&filing.origin)
            .bind(&filing.reason)
            .bind(filing.rules_version)
            .bind(filing.now.timestamp())
            .execute(&mut **tx)
            .await?;
            (filing.id.clone(), true)
        }
    };
    let mut changed = inserted;
    for link in &filing.links {
        let result = sqlx::query(
            "INSERT INTO record_messages
                (record_id, message_id, thread_id, stage, message_at, filed_by, filed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(record_id, message_id) DO UPDATE SET
                stage = excluded.stage, thread_id = excluded.thread_id
             WHERE record_messages.stage <> excluded.stage",
        )
        .bind(&record_id)
        .bind(link.message_id.as_str())
        .bind(link.thread_id.as_ref().map(ThreadId::as_str))
        .bind(&link.stage)
        .bind(link.message_at.timestamp())
        .bind(&link.filed_by)
        .bind(filing.now.timestamp())
        .execute(&mut **tx)
        .await?;
        changed |= result.rows_affected() > 0;
    }
    for value in &filing.fields {
        changed |= upsert_field_in_tx(tx, &record_id, value, true).await?;
    }
    if changed {
        if !inserted {
            // A stronger origin (schema over rule) updates the why line.
            sqlx::query(
                "UPDATE records SET updated_at = ?2,
                    origin = CASE WHEN ?3 = 'schema' THEN ?3 ELSE origin END,
                    reason = CASE WHEN ?3 = 'schema' THEN ?4 ELSE reason END
                 WHERE id = ?1",
            )
            .bind(&record_id)
            .bind(filing.now.timestamp())
            .bind(&filing.origin)
            .bind(&filing.reason)
            .execute(&mut **tx)
            .await?;
        }
        recompute_in_tx(tx, &[record_id.as_str()]).await?;
    }
    Ok(if inserted {
        RecordFiled::Inserted { id: record_id }
    } else if changed {
        RecordFiled::Updated { id: record_id }
    } else {
        RecordFiled::Unchanged { id: record_id }
    })
}

async fn read_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record_id: &str,
) -> Result<Option<(ArchiveRecord, Vec<RecordFieldValue>)>, sqlx::Error> {
    let sql = format!("SELECT {RECORD_COLUMNS} FROM records WHERE id = ?");
    let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(record_id)
        .fetch_optional(&mut **tx)
        .await?
    else {
        return Ok(None);
    };
    let record = row_to_record(&row)?;
    let fields = sqlx::query(
        "SELECT * FROM record_fields WHERE record_id = ?
         ORDER BY field, rank DESC, observed_at DESC, source_key DESC",
    )
    .bind(record_id)
    .fetch_all(&mut **tx)
    .await?
    .iter()
    .map(|row| row_to_field(row).map(|(_, field)| field))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((record, fields)))
}

impl super::Store {
    /// Files a record: inserts it or adds to the one with the same dedup
    /// key, links its emails and writes its field candidates, then
    /// recomputes the winning values. A record dismissed as not a record is
    /// left alone.
    pub async fn file_record(&self, filing: &RecordFiling) -> Result<RecordFiled, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let filed = file_record_in_tx(&mut tx, filing).await?;
        tx.commit().await?;
        Ok(filed)
    }

    /// What [`Self::file_record`] would leave: the record and its field
    /// candidates, read inside a transaction that is rolled back. The dry
    /// run of a manual filing shows exactly what the real one writes.
    pub async fn preview_file_record(
        &self,
        filing: &RecordFiling,
    ) -> Result<Option<(ArchiveRecord, Vec<RecordFieldValue>)>, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let filed = file_record_in_tx(&mut tx, filing).await?;
        let preview = read_in_tx(&mut tx, filed.id()).await?;
        tx.rollback().await?;
        Ok(preview)
    }

    pub async fn get_archive_record(&self, id: &str) -> Result<Option<ArchiveRecord>, sqlx::Error> {
        let sql = format!("SELECT {RECORD_COLUMNS} FROM records WHERE id = ?");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(row_to_record)
            .transpose()
    }

    pub async fn get_archive_records(
        &self,
        ids: &[String],
    ) -> Result<Vec<ArchiveRecord>, sqlx::Error> {
        fetch_by_ids(
            self.reader(),
            &format!("SELECT {RECORD_COLUMNS} FROM records WHERE id IN ({{}})"),
            ids,
        )
        .await?
        .iter()
        .map(row_to_record)
        .collect()
    }

    pub async fn get_archive_record_by_dedup(
        &self,
        account_id: &AccountId,
        dedup_key: &str,
    ) -> Result<Option<ArchiveRecord>, sqlx::Error> {
        let sql =
            format!("SELECT {RECORD_COLUMNS} FROM records WHERE account_id = ? AND dedup_key = ?");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(dedup_key)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(row_to_record)
            .transpose()
    }

    /// Record ids starting with `prefix`, at most `limit`.
    pub async fn find_record_ids_by_prefix(
        &self,
        prefix: &str,
        limit: u32,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT id FROM records WHERE id >= ?1 AND id < ?1 || '~' ORDER BY id LIMIT ?2",
        )
        .bind(prefix)
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await
    }

    /// The order or booking with this reference, by any issuer name, so a
    /// shipping email from a carrier joins the shop's order.
    pub async fn find_record_by_reference(
        &self,
        account_id: &AccountId,
        kinds: &[&str],
        reference: &str,
    ) -> Result<Option<ArchiveRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM records
             WHERE account_id = ? AND reference = ? COLLATE NOCASE AND kind IN ({})
             ORDER BY created_at LIMIT 1",
            in_list(kinds.len())
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(reference);
        for kind in kinds {
            query = query.bind(*kind);
        }
        query
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(row_to_record)
            .transpose()
    }

    /// Records matching `query`, newest ledger date first. Unbounded: the
    /// handler builds month totals and facets over the whole set.
    pub async fn list_archive_records(
        &self,
        query: &RecordQuery,
    ) -> Result<Vec<ArchiveRecord>, sqlx::Error> {
        let mut conditions = Vec::new();
        let mut binds: Vec<Bind> = Vec::new();
        if !query.include_dismissed {
            conditions.push("dismissed_at IS NULL".to_string());
        }
        if let Some(accounts) = &query.account_ids {
            if accounts.is_empty() {
                return Ok(Vec::new());
            }
            conditions.push(format!("account_id IN ({})", in_list(accounts.len())));
            binds.extend(accounts.iter().map(|a| Bind::Text(a.as_str())));
        }
        if !query.kinds.is_empty() {
            conditions.push(format!("kind IN ({})", in_list(query.kinds.len())));
            binds.extend(query.kinds.iter().cloned().map(Bind::Text));
        }
        if let Some(issuer) = &query.issuer_key {
            conditions.push("issuer_key = ?".to_string());
            binds.push(Bind::Text(issuer.clone()));
        }
        if let Some(from) = query.from {
            conditions.push("COALESCE(issued_at, last_message_at) >= ?".to_string());
            binds.push(Bind::Int(from.timestamp()));
        }
        if let Some(until) = query.until {
            conditions.push("COALESCE(issued_at, last_message_at) < ?".to_string());
            binds.push(Bind::Int(until.timestamp()));
        }
        if let Some(min) = query.min_amount_minor {
            conditions.push("amount_minor >= ?".to_string());
            binds.push(Bind::Int(min));
        }
        if let Some(max) = query.max_amount_minor {
            conditions.push("amount_minor <= ?".to_string());
            binds.push(Bind::Int(max));
        }
        if let Some(checked) = query.checked {
            conditions.push("checked = ?".to_string());
            binds.push(Bind::Int(i64::from(checked)));
        }
        if let Some(group) = &query.group_id {
            conditions.push("group_id = ?".to_string());
            binds.push(Bind::Text(group.clone()));
        }
        if let Some(after) = query.moment_after {
            conditions.push(
                "MAX(COALESCE(span_start, 0), COALESCE(span_end, 0), COALESCE(return_by, 0),
                     COALESCE(warranty_until, 0)) >= ?"
                    .to_string(),
            );
            binds.push(Bind::Int(after.timestamp()));
        }
        if let Some(has_pdf) = query.has_pdf {
            let exists = "EXISTS (SELECT 1 FROM record_messages rm
                 JOIN attachments a ON a.message_id = rm.message_id
                 WHERE rm.record_id = records.id
                   AND (LOWER(a.mime_type) = 'application/pdf' OR LOWER(a.filename) LIKE '%.pdf'))";
            conditions.push(if has_pdf {
                exists.to_string()
            } else {
                format!("NOT {exists}")
            });
        }
        let filter = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM records {filter}
             ORDER BY COALESCE(issued_at, last_message_at) DESC, id DESC"
        );
        let mut sqlx_query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for bind in binds {
            sqlx_query = match bind {
                Bind::Text(text) => sqlx_query.bind(text),
                Bind::Int(int) => sqlx_query.bind(int),
            };
        }
        sqlx_query
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_record)
            .collect()
    }

    /// Every field candidate of these records, by record id.
    pub async fn archive_record_fields(
        &self,
        record_ids: &[String],
    ) -> Result<Vec<(String, RecordFieldValue)>, sqlx::Error> {
        let ids = record_ids;
        fetch_by_ids(
            self.reader(),
            "SELECT * FROM record_fields WHERE record_id IN ({})
             ORDER BY record_id, field, rank DESC, observed_at DESC, source_key DESC",
            ids,
        )
        .await?
        .iter()
        .map(row_to_field)
        .collect()
    }

    /// The source emails of these records, oldest first.
    pub async fn archive_record_sources(
        &self,
        record_ids: &[String],
    ) -> Result<Vec<RecordSource>, sqlx::Error> {
        let ids = record_ids;
        fetch_by_ids(
            self.reader(),
            "SELECT rm.record_id, rm.message_id, rm.thread_id, rm.stage, rm.message_at,
                    rm.filed_by, m.subject
             FROM record_messages rm JOIN messages m ON m.id = rm.message_id
             WHERE rm.record_id IN ({})
             ORDER BY rm.record_id, rm.message_at, rm.message_id",
            ids,
        )
        .await?
        .iter()
        .map(|row| {
            Ok(RecordSource {
                record_id: row.try_get("record_id")?,
                message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
                thread_id: row
                    .try_get::<Option<String>, _>("thread_id")?
                    .map(|id| decode_id(&id))
                    .transpose()?,
                stage: row.try_get("stage")?,
                message_at: decode_timestamp(row.try_get("message_at")?)?,
                filed_by: row.try_get("filed_by")?,
                subject: row.try_get("subject")?,
            })
        })
        .collect()
    }

    /// The attachments of these records' source emails, PDFs first.
    pub async fn archive_record_documents(
        &self,
        record_ids: &[String],
    ) -> Result<Vec<RecordDocument>, sqlx::Error> {
        let ids = record_ids;
        fetch_by_ids(
            self.reader(),
            "SELECT rm.record_id, a.message_id, a.id AS attachment_id, a.filename, a.mime_type,
                    a.size_bytes, a.local_path
             FROM record_messages rm JOIN attachments a ON a.message_id = rm.message_id
             WHERE rm.record_id IN ({})
             ORDER BY rm.record_id,
                      CASE WHEN LOWER(a.mime_type) = 'application/pdf'
                                OR LOWER(a.filename) LIKE '%.pdf' THEN 0 ELSE 1 END,
                      rm.message_at DESC, a.filename",
            ids,
        )
        .await?
        .iter()
        .map(|row| {
            Ok(RecordDocument {
                record_id: row.try_get("record_id")?,
                message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
                attachment_id: row.try_get("attachment_id")?,
                filename: row.try_get("filename")?,
                mime_type: row.try_get("mime_type")?,
                size_bytes: row.try_get("size_bytes")?,
                local_path: row.try_get("local_path")?,
            })
        })
        .collect()
    }

    /// The records these messages are a source of.
    pub async fn archive_record_ids_for_messages(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<(MessageId, String)>, sqlx::Error> {
        let ids: Vec<String> = message_ids.iter().map(MessageId::as_str).collect();
        fetch_by_ids(
            self.reader(),
            "SELECT message_id, record_id FROM record_messages WHERE message_id IN ({})",
            &ids,
        )
        .await?
        .iter()
        .map(|row| {
            Ok((
                decode_id(&row.try_get::<String, _>("message_id")?)?,
                row.try_get("record_id")?,
            ))
        })
        .collect()
    }

    /// The records filed from these threads, not dismissed.
    pub async fn archive_records_for_threads(
        &self,
        thread_ids: &[ThreadId],
    ) -> Result<Vec<(ThreadId, ArchiveRecord)>, sqlx::Error> {
        let ids: Vec<String> = thread_ids.iter().map(ThreadId::as_str).collect();
        let template = format!(
            "SELECT DISTINCT rm.thread_id AS source_thread, {} FROM record_messages rm
             JOIN records r ON r.id = rm.record_id
             WHERE rm.thread_id IN ({{}}) AND r.dismissed_at IS NULL
             ORDER BY rm.thread_id, r.created_at, r.id",
            RECORD_COLUMNS
                .split(',')
                .map(|column| format!("r.{}", column.trim()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        fetch_by_ids(self.reader(), &template, &ids)
            .await?
            .iter()
            .map(|row| {
                Ok((
                    decode_id(&row.try_get::<String, _>("source_thread")?)?,
                    row_to_record(row)?,
                ))
            })
            .collect()
    }

    /// Applies the user's edit and recomputes the record. With `commit:
    /// false` the edit runs inside a transaction that is rolled back, so a
    /// dry run returns exactly what the real edit leaves. `None` when the
    /// record doesn't exist.
    pub async fn edit_archive_record(
        &self,
        record_id: &str,
        edit: &RecordFieldEdit,
        now: DateTime<Utc>,
        commit: bool,
    ) -> Result<Option<(ArchiveRecord, Vec<RecordFieldValue>)>, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM records WHERE id = ?")
            .bind(record_id)
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            tx.rollback().await?;
            return Ok(None);
        }
        let user =
            |field: &str, value_text: Option<String>, value_int: Option<i64>, evidence: &str| {
                RecordFieldValue {
                    field: field.to_string(),
                    source_key: "user".to_string(),
                    message_id: None,
                    source: "user".to_string(),
                    rank: 4,
                    value_text,
                    value_int,
                    checked: true,
                    // A user row copies no text from mail, so it never outlives
                    // a delete with words from it.
                    evidence: Some(evidence.to_string()),
                    observed_at: now,
                }
            };
        let confirm_fields: Vec<String> = match edit {
            RecordFieldEdit::Set {
                field,
                value_text,
                value_int,
            } => {
                upsert_field_in_tx(
                    &mut tx,
                    record_id,
                    &user(field, value_text.clone(), *value_int, "you"),
                    false,
                )
                .await?;
                Vec::new()
            }
            RecordFieldEdit::Clear { field } => {
                sqlx::query(
                    "DELETE FROM record_fields
                     WHERE record_id = ?1 AND field = ?2 AND source_key = 'user'",
                )
                .bind(record_id)
                .bind(field)
                .execute(&mut *tx)
                .await?;
                Vec::new()
            }
            RecordFieldEdit::Confirm { field } => vec![field.clone()],
            RecordFieldEdit::ConfirmUnchecked => {
                let sql = format!(
                    "SELECT DISTINCT field FROM record_fields
                     WHERE record_id = ? AND field IN ({})",
                    in_list(RECORD_CHECKED_FIELDS.len())
                );
                let mut query =
                    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql)).bind(record_id);
                for field in RECORD_CHECKED_FIELDS {
                    query = query.bind(*field);
                }
                query.fetch_all(&mut *tx).await?
            }
        };
        for field in &confirm_fields {
            let winner: Option<(Option<String>, Option<i64>, i64)> = sqlx::query_as(
                "SELECT value_text, value_int, checked FROM record_fields
                 WHERE record_id = ?1 AND field = ?2
                 ORDER BY rank DESC, observed_at DESC, source_key DESC LIMIT 1",
            )
            .bind(record_id)
            .bind(field)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((text, int, checked)) = winner else {
                continue;
            };
            // Confirming everything leaves checked values as they are.
            if matches!(edit, RecordFieldEdit::ConfirmUnchecked) && checked != 0 {
                continue;
            }
            upsert_field_in_tx(
                &mut tx,
                record_id,
                &user(field, text, int, "you confirmed"),
                false,
            )
            .await?;
        }
        sqlx::query("UPDATE records SET updated_at = ?2 WHERE id = ?1")
            .bind(record_id)
            .bind(now.timestamp())
            .execute(&mut *tx)
            .await?;
        recompute_in_tx(&mut tx, &[record_id]).await?;
        let after = read_in_tx(&mut tx, record_id).await?;
        if commit {
            tx.commit().await?;
        } else {
            tx.rollback().await?;
        }
        Ok(after)
    }

    /// Records with a source email from `sender_email`, for corrections
    /// that apply to a sender.
    pub async fn archive_record_ids_from_sender(
        &self,
        account_id: &AccountId,
        sender_email: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT DISTINCT rm.record_id FROM record_messages rm
             JOIN messages m ON m.id = rm.message_id
             WHERE m.account_id = ?1 AND LOWER(m.from_email) = LOWER(?2)",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .fetch_all(self.reader())
        .await
    }

    /// The account's inbound messages from `sender_email`, newest first.
    pub async fn message_ids_from_sender(
        &self,
        account_id: &AccountId,
        sender_email: &str,
        limit: u32,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM messages
             WHERE account_id = ?1 AND LOWER(from_email) = LOWER(?2) AND direction <> 'outbound'
             ORDER BY date DESC LIMIT ?3",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?
        .iter()
        .map(|id| decode_id(id))
        .collect()
    }

    /// Not a record (`dismiss: true`) or back again. Returns the ids that
    /// changed.
    pub async fn set_archive_records_dismissed(
        &self,
        record_ids: &[String],
        dismiss: bool,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        let (set, unchanged) = if dismiss {
            ("dismissed_at = ?1", "dismissed_at IS NULL")
        } else {
            ("dismissed_at = NULL", "dismissed_at IS NOT NULL")
        };
        let mut tx = self.writer().begin().await?;
        let mut changed = Vec::new();
        for chunk in record_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "UPDATE records SET {set}, updated_at = ?1
                 WHERE {unchanged} AND id IN ({}) RETURNING id",
                crate::in_list_after_first(chunk.len())
            );
            let mut query =
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql)).bind(now.timestamp());
            for id in chunk {
                query = query.bind(id);
            }
            changed.extend(query.fetch_all(&mut *tx).await?);
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Removes what a ticked-off to-do filed: its field candidates, and the
    /// record itself when the to-do made it. Undo of a tick-off.
    pub async fn unfile_todo_record(
        &self,
        account_id: &AccountId,
        todo_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let source_key = todo_record_source_key(todo_id);
        let touched: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT record_id FROM record_fields WHERE source_key = ?")
                .bind(&source_key)
                .fetch_all(&mut *tx)
                .await?;
        sqlx::query("DELETE FROM record_fields WHERE source_key = ?")
            .bind(&source_key)
            .execute(&mut *tx)
            .await?;
        let removed = sqlx::query("DELETE FROM records WHERE account_id = ? AND dedup_key = ?")
            .bind(account_id.as_str())
            .bind(todo_record_dedup_key(todo_id))
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let touched: Vec<&str> = touched.iter().map(String::as_str).collect();
        recompute_in_tx(&mut tx, &touched).await?;
        tx.commit().await?;
        Ok(removed)
    }

    /// Deletes a record the user filed by hand, with its links. Undo of a
    /// manual filing.
    pub async fn delete_archive_record(&self, record_id: &str) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query("DELETE FROM records WHERE id = ?")
            .bind(record_id)
            .execute(self.writer())
            .await?
            .rows_affected()
            > 0)
    }

    // ----- Groups -----

    /// Every group of an account.
    pub async fn list_record_groups(
        &self,
        account_ids: Option<&[AccountId]>,
    ) -> Result<Vec<RecordGroup>, sqlx::Error> {
        let (filter, ids): (String, Vec<String>) = match account_ids {
            Some([]) => return Ok(Vec::new()),
            Some(ids) => (
                format!("WHERE account_id IN ({})", in_list(ids.len())),
                ids.iter().map(AccountId::as_str).collect(),
            ),
            None => (String::new(), Vec::new()),
        };
        let sql = format!(
            "SELECT id, account_id, kind, group_key, title, span_start, span_end
             FROM record_groups {filter}"
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in ids {
            query = query.bind(id);
        }
        query
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(|row| {
                Ok(RecordGroup {
                    id: row.try_get("id")?,
                    account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
                    kind: row.try_get("kind")?,
                    group_key: row.try_get("group_key")?,
                    title: row.try_get("title")?,
                    span_start: decode_optional_timestamp(row.try_get("span_start")?)?,
                    span_end: decode_optional_timestamp(row.try_get("span_end")?)?,
                })
            })
            .collect()
    }

    /// How many records (not dismissed) each of these groups holds.
    pub async fn record_group_counts(
        &self,
        group_ids: &[String],
    ) -> Result<std::collections::HashMap<String, u32>, sqlx::Error> {
        let rows = fetch_by_ids(
            self.reader(),
            "SELECT group_id, COUNT(*) AS n FROM records
             WHERE group_id IN ({}) AND dismissed_at IS NULL GROUP BY group_id",
            group_ids,
        )
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("group_id")?,
                    u32::try_from(row.try_get::<i64, _>("n")?).unwrap_or(u32::MAX),
                ))
            })
            .collect()
    }

    /// Replaces an account's groups with `groups`, each with its member
    /// record ids, in one transaction. Groups whose key is unchanged keep
    /// their id, so a link to a trip survives a regroup.
    pub async fn replace_record_groups(
        &self,
        account_id: &AccountId,
        groups: &[(RecordGroup, Vec<String>)],
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        sqlx::query(
            "UPDATE records SET group_id = NULL WHERE account_id = ? AND group_id IS NOT NULL",
        )
        .bind(account_id.as_str())
        .execute(&mut *tx)
        .await?;
        let mut kept = Vec::new();
        for (group, members) in groups {
            let id: String = sqlx::query_scalar(
                "INSERT INTO record_groups
                    (id, account_id, kind, group_key, title, span_start, span_end, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(account_id, group_key) DO UPDATE SET
                    title = excluded.title, span_start = excluded.span_start,
                    span_end = excluded.span_end, updated_at = excluded.updated_at
                 RETURNING id",
            )
            .bind(&group.id)
            .bind(account_id.as_str())
            .bind(&group.kind)
            .bind(&group.group_key)
            .bind(&group.title)
            .bind(group.span_start.map(|at| at.timestamp()))
            .bind(group.span_end.map(|at| at.timestamp()))
            .bind(now.timestamp())
            .fetch_one(&mut *tx)
            .await?;
            for member in members {
                sqlx::query("UPDATE records SET group_id = ?1 WHERE id = ?2 AND account_id = ?3")
                    .bind(&id)
                    .bind(member)
                    .bind(account_id.as_str())
                    .execute(&mut *tx)
                    .await?;
            }
            kept.push(id);
        }
        let sql = if kept.is_empty() {
            "DELETE FROM record_groups WHERE account_id = ?".to_string()
        } else {
            format!(
                "DELETE FROM record_groups WHERE account_id = ? AND id NOT IN ({})",
                in_list(kept.len())
            )
        };
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(account_id.as_str());
        for id in &kept {
            query = query.bind(id);
        }
        query.execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    // ----- Senders -----

    pub async fn record_sender_rules(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<RecordSenderRule>, sqlx::Error> {
        sqlx::query(
            "SELECT account_id, sender_email, verdict, kind, issuer_name, decided_at
             FROM record_senders WHERE account_id = ?",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?
        .iter()
        .map(|row| {
            Ok(RecordSenderRule {
                account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
                sender_email: row.try_get("sender_email")?,
                verdict: row.try_get("verdict")?,
                kind: row.try_get("kind")?,
                issuer_name: row.try_get("issuer_name")?,
                decided_at: decode_timestamp(row.try_get("decided_at")?)?,
            })
        })
        .collect()
    }

    /// Sets a sender's verdict and kind (`verdict: None` clears it), keeping
    /// any issuer name.
    pub async fn set_record_sender_verdict(
        &self,
        account_id: &AccountId,
        sender_email: &str,
        verdict: Option<&str>,
        kind: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO record_senders (account_id, sender_email, verdict, kind, decided_at)
             VALUES (?1, LOWER(?2), ?3, ?4, ?5)
             ON CONFLICT(account_id, sender_email) DO UPDATE SET
                verdict = excluded.verdict, kind = excluded.kind, decided_at = excluded.decided_at",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .bind(verdict)
        .bind(kind)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Names a sender's issuer for every record from them, now and later.
    pub async fn set_record_sender_issuer(
        &self,
        account_id: &AccountId,
        sender_email: &str,
        issuer_name: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO record_senders (account_id, sender_email, issuer_name, decided_at)
             VALUES (?1, LOWER(?2), ?3, ?4)
             ON CONFLICT(account_id, sender_email) DO UPDATE SET
                issuer_name = excluded.issuer_name, decided_at = excluded.decided_at",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .bind(issuer_name)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    // ----- First run -----

    pub async fn get_record_run(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<RecordRun>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT rules_version, cursor_date, cursor_message_id, scanned, completed_at
             FROM record_runs WHERE account_id = ?",
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| {
            let cursor = match (
                row.try_get::<Option<i64>, _>("cursor_date")?,
                row.try_get::<Option<String>, _>("cursor_message_id")?,
            ) {
                (Some(date), Some(id)) => Some((decode_timestamp(date)?, decode_id(&id)?)),
                _ => None,
            };
            Ok(RecordRun {
                rules_version: row.try_get("rules_version")?,
                cursor,
                scanned: row.try_get("scanned")?,
                completed_at: decode_optional_timestamp(row.try_get("completed_at")?)?,
            })
        })
        .transpose()
    }

    /// Starts (or restarts, for a new rules version) an account's run.
    pub async fn start_record_run(
        &self,
        account_id: &AccountId,
        rules_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO record_runs (account_id, rules_version, scanned, started_at)
             VALUES (?1, ?2, 0, ?3)
             ON CONFLICT(account_id) DO UPDATE SET
                rules_version = excluded.rules_version, cursor_date = NULL,
                cursor_message_id = NULL, scanned = 0, started_at = excluded.started_at,
                completed_at = NULL",
        )
        .bind(account_id.as_str())
        .bind(rules_version)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn advance_record_run(
        &self,
        account_id: &AccountId,
        cursor: &(DateTime<Utc>, MessageId),
        scanned: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE record_runs SET cursor_date = ?2, cursor_message_id = ?3,
                scanned = scanned + ?4 WHERE account_id = ?1",
        )
        .bind(account_id.as_str())
        .bind(cursor.0.timestamp())
        .bind(cursor.1.as_str())
        .bind(scanned)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn complete_record_run(
        &self,
        account_id: &AccountId,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE record_runs SET completed_at = ?2 WHERE account_id = ?1")
            .bind(account_id.as_str())
            .bind(now.timestamp())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Delivered parcels of an account (not dismissed) whose delivery no
    /// record carries yet, oldest first.
    pub async fn delivered_deliveries_to_file(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<mxr_core::id::DeliveryId>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT d.id FROM deliveries d
             WHERE d.account_id = ?1 AND d.delivered_at IS NOT NULL AND d.dismissed_at IS NULL
               AND NOT EXISTS (
                   SELECT 1 FROM record_fields rf
                   JOIN delivery_messages dm ON dm.message_id = rf.message_id
                   WHERE dm.delivery_id = d.id AND rf.source = 'delivery'
                     AND rf.field = 'delivered_at' AND rf.value_int = d.delivered_at)
             ORDER BY d.delivered_at",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?
        .iter()
        .map(|id| decode_id(id))
        .collect()
    }

    // ----- PDF prefetch -----

    /// PDFs of filed records' emails that are not on disk, newest record
    /// first, each at most `max_bytes`.
    pub async fn record_pdfs_to_fetch(
        &self,
        max_bytes: i64,
        limit: u32,
    ) -> Result<Vec<RecordPdfToFetch>, sqlx::Error> {
        sqlx::query(
            "SELECT DISTINCT a.message_id, a.id AS attachment_id, a.size_bytes, rm.message_at
             FROM record_messages rm
             JOIN records r ON r.id = rm.record_id AND r.dismissed_at IS NULL
             JOIN attachments a ON a.message_id = rm.message_id
             WHERE a.local_path IS NULL
               AND (LOWER(a.mime_type) = 'application/pdf' OR LOWER(a.filename) LIKE '%.pdf')
               AND a.size_bytes <= ?1
             ORDER BY rm.message_at DESC
             LIMIT ?2",
        )
        .bind(max_bytes)
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?
        .iter()
        .map(|row| {
            Ok(RecordPdfToFetch {
                message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
                attachment_id: row.try_get("attachment_id")?,
                size_bytes: row.try_get("size_bytes")?,
            })
        })
        .collect()
    }

    /// Bytes and count of record PDFs already on disk: what the prefetch
    /// budget is measured against.
    pub async fn record_pdfs_on_disk(&self) -> Result<(i64, i64), sqlx::Error> {
        let row: (Option<i64>, i64) = sqlx::query_as(
            "SELECT SUM(size_bytes), COUNT(*) FROM attachments
             WHERE local_path IS NOT NULL
               AND (LOWER(mime_type) = 'application/pdf' OR LOWER(filename) LIKE '%.pdf')
               AND message_id IN (
                   SELECT rm.message_id FROM record_messages rm
                   JOIN records r ON r.id = rm.record_id AND r.dismissed_at IS NULL)",
        )
        .fetch_one(self.reader())
        .await?;
        Ok((row.0.unwrap_or(0), row.1))
    }

    /// One line of field text per message that is a record's source, for
    /// Archive's index recipe: kind, issuer, title, reference, place.
    pub async fn record_field_text_for_messages(
        &self,
        message_ids: &[MessageId],
    ) -> Result<std::collections::HashMap<MessageId, String>, sqlx::Error> {
        let ids: Vec<String> = message_ids.iter().map(MessageId::as_str).collect();
        let rows = fetch_by_ids(
            self.reader(),
            "SELECT rm.message_id, r.kind, r.issuer, r.title, r.reference, r.place
             FROM record_messages rm JOIN records r ON r.id = rm.record_id
             WHERE rm.message_id IN ({}) AND r.dismissed_at IS NULL",
            &ids,
        )
        .await?;
        let mut out: std::collections::HashMap<MessageId, String> =
            std::collections::HashMap::new();
        for row in rows {
            let id: MessageId = decode_id(&row.try_get::<String, _>("message_id")?)?;
            let parts: Vec<String> = ["kind", "issuer", "title", "reference", "place"]
                .iter()
                .filter_map(|column| row.try_get::<Option<String>, _>(*column).ok().flatten())
                .filter(|value| !value.trim().is_empty())
                .collect();
            if parts.is_empty() {
                continue;
            }
            let line = format!("record {}", parts.join(" "));
            out.entry(id)
                .and_modify(|existing| {
                    existing.push(' ');
                    existing.push_str(&line);
                })
                .or_insert(line);
        }
        Ok(out)
    }

    /// Records (not dismissed) of these accounts with a PDF among their
    /// emails' attachments.
    pub async fn count_archive_records_with_pdf(
        &self,
        account_ids: &[AccountId],
    ) -> Result<u32, sqlx::Error> {
        if account_ids.is_empty() {
            return Ok(0);
        }
        let sql = format!(
            "SELECT COUNT(*) FROM records
             WHERE dismissed_at IS NULL AND account_id IN ({})
               AND EXISTS (SELECT 1 FROM record_messages rm
                   JOIN attachments a ON a.message_id = rm.message_id
                   WHERE rm.record_id = records.id
                     AND (LOWER(a.mime_type) = 'application/pdf' OR LOWER(a.filename) LIKE '%.pdf'))",
            in_list(account_ids.len())
        );
        let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
        for id in account_ids {
            query = query.bind(id.as_str());
        }
        Ok(u32::try_from(query.fetch_one(self.reader()).await?).unwrap_or(u32::MAX))
    }

    /// Records not dismissed, by kind, for counts.
    pub async fn count_archive_records_by_kind(
        &self,
        account_ids: Option<&[AccountId]>,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let (filter, ids): (String, Vec<String>) = match account_ids {
            Some([]) => return Ok(Vec::new()),
            Some(ids) => (
                format!("AND account_id IN ({})", in_list(ids.len())),
                ids.iter().map(AccountId::as_str).collect(),
            ),
            None => (String::new(), Vec::new()),
        };
        let sql = format!(
            "SELECT kind, COUNT(*) AS n FROM records WHERE dismissed_at IS NULL {filter}
             GROUP BY kind ORDER BY n DESC"
        );
        let mut query = sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(sql));
        for id in ids {
            query = query.bind(id);
        }
        query.fetch_all(self.reader()).await
    }
}

enum Bind {
    Text(String),
    Int(i64),
}

#[cfg(test)]
mod tests;
