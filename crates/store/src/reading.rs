//! Reading's storage (`066_reading.sql`): the extraction cache, what you
//! did with each item (Later, engagement), fetched articles, highlights,
//! per-source choices and the visit that bounds "Since you were last here".
//!
//! The daemon decides everything; these are reads and writes only.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};
use sqlx::Row;
use std::collections::HashMap;

/// One cached readable item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingItemRow {
    pub message_id: MessageId,
    /// 0 is the issue; a digest's link is keyed by its normalised URL.
    pub idx: i64,
    /// Display order within the issue.
    pub position: i64,
    pub account_id: AccountId,
    /// `issue` or `link`.
    pub kind: String,
    /// `single`, `digest`, `teaser` or `notice`.
    pub shape: String,
    pub title: String,
    pub standfirst: Option<String>,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub tracked: bool,
    pub words: u32,
    pub extractor_version: u32,
}

/// What you did with one item.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadingStateRow {
    pub message_id: MessageId,
    pub idx: i64,
    pub account_id: AccountId,
    pub later_at: Option<DateTime<Utc>>,
    pub kept_at: Option<DateTime<Utc>>,
    pub opened_at: Option<DateTime<Utc>>,
    pub dwell_ms: u64,
    pub progress: f64,
    pub finished_at: Option<DateTime<Utc>>,
}

/// One of a source's recent issues, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIssue {
    pub from_email: String,
    pub message_id: MessageId,
    pub date: DateTime<Utc>,
    pub opened: bool,
    pub finished: bool,
}

/// A fetched article, or the failure to fetch one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingArticleRow {
    pub message_id: MessageId,
    pub idx: i64,
    pub url: String,
    pub final_url: Option<String>,
    /// `ok` or `failed`.
    pub status: String,
    pub title: Option<String>,
    pub byline: Option<String>,
    pub site_name: Option<String>,
    pub html: Option<String>,
    /// JSON paragraphs.
    pub paragraphs: Option<String>,
    pub words: u32,
    /// JSON list of hosts contacted.
    pub contacted: String,
    pub error: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadingHighlightRow {
    pub id: String,
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub idx: i64,
    /// `issue` or `article`.
    pub view: String,
    pub quote: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    /// `mxr`, or `marginalia` for one read back from Marginalia.
    pub origin: String,
    /// The highlight's id where it came from; set for `marginalia`.
    pub origin_id: Option<String>,
}

/// A Reading item sent to Marginalia (`086_reading_marginalia.sql`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingMarginaliaRow {
    pub message_id: MessageId,
    pub idx: i64,
    pub account_id: AccountId,
    pub article_id: String,
    pub read_url: String,
    /// `issue` or `article`: the text that was sent.
    pub view: String,
    pub sent_at: DateTime<Utc>,
    pub highlights_synced_at: Option<DateTime<Utc>>,
}

/// One highlight as Marginalia has it, to mirror locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarginaliaHighlightIn {
    pub origin_id: String,
    pub quote: String,
    pub created_at: DateTime<Utc>,
}

/// What a note edit or removal did to one highlight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HighlightEdit {
    /// It changed; this is the highlight as it was.
    Done(ReadingHighlightRow),
    /// No such highlight (already removed, say).
    Missing,
    /// It was made elsewhere (carries its origin) and is read back, so a
    /// local change would be undone or fought by the next read-back.
    ReadOnly(String),
}

/// The only origin mxr may change: its own. Anything else is mirrored from
/// elsewhere, so this is an allowlist of one, not a refusal of `marginalia`.
pub const LOCAL_HIGHLIGHT_ORIGIN: &str = "mxr";

/// What mirroring one item's Marginalia highlights changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarginaliaMirror {
    pub added: u32,
    pub removed: u32,
}

/// Per-source choices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadingSourcePrefs {
    pub original_layout: bool,
    pub unsubscribe_offer_dismissed: bool,
}

/// One engagement report from a client.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReadingEngagementReport {
    pub opened: bool,
    pub dwell_ms: u64,
    pub progress: f64,
}

/// When Reading was opened: the first time (engagement counts from
/// there), the end of the previous visit, and the latest open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadingVisitRow {
    pub first_seen: Option<DateTime<Utc>>,
    pub boundary: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

/// `?3, ?4, ...`: numbered, for a list after numbered parameters.
fn numbered(first: usize, n: usize) -> String {
    (first..first + n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn item_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ReadingItemRow, sqlx::Error> {
    Ok(ReadingItemRow {
        message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
        idx: row.try_get("idx")?,
        position: row.try_get("position")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        kind: row.try_get("kind")?,
        shape: row.try_get("shape")?,
        title: row.try_get("title")?,
        standfirst: row.try_get("standfirst")?,
        url: row.try_get("url")?,
        domain: row.try_get("domain")?,
        tracked: row.try_get::<i64, _>("tracked")? != 0,
        words: u32::try_from(row.try_get::<i64, _>("words")?).unwrap_or(0),
        extractor_version: u32::try_from(row.try_get::<i64, _>("extractor_version")?).unwrap_or(0),
    })
}

fn state_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ReadingStateRow, sqlx::Error> {
    Ok(ReadingStateRow {
        message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
        idx: row.try_get("idx")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        later_at: decode_optional_timestamp(row.try_get("later_at")?)?,
        kept_at: decode_optional_timestamp(row.try_get("kept_at")?)?,
        opened_at: decode_optional_timestamp(row.try_get("opened_at")?)?,
        dwell_ms: u64::try_from(row.try_get::<i64, _>("dwell_ms")?).unwrap_or(0),
        progress: row.try_get("progress")?,
        finished_at: decode_optional_timestamp(row.try_get("finished_at")?)?,
    })
}

fn article_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ReadingArticleRow, sqlx::Error> {
    Ok(ReadingArticleRow {
        message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
        idx: row.try_get("idx")?,
        url: row.try_get("url")?,
        final_url: row.try_get("final_url")?,
        status: row.try_get("status")?,
        title: row.try_get("title")?,
        byline: row.try_get("byline")?,
        site_name: row.try_get("site_name")?,
        html: row.try_get("html")?,
        paragraphs: row.try_get("paragraphs")?,
        words: u32::try_from(row.try_get::<i64, _>("words")?).unwrap_or(0),
        contacted: row.try_get("contacted")?,
        error: row.try_get("error")?,
        fetched_at: decode_timestamp(row.try_get("fetched_at")?)?,
    })
}

fn highlight_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ReadingHighlightRow, sqlx::Error> {
    Ok(ReadingHighlightRow {
        id: row.try_get("id")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
        idx: row.try_get("idx")?,
        view: row.try_get("view")?,
        quote: row.try_get("quote")?,
        note: row.try_get("note")?,
        created_at: decode_timestamp(row.try_get("created_at")?)?,
        origin: row.try_get("origin")?,
        origin_id: row.try_get("origin_id")?,
    })
}

/// The highlight, read inside the transaction that will change it, unless
/// it is missing or not mxr's own (then the refusal to hand back).
async fn own_highlight_in(
    tx: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<Result<ReadingHighlightRow, HighlightEdit>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM reading_highlights WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .as_ref()
        .map(highlight_from_row)
        .transpose()?;
    Ok(match row {
        None => Err(HighlightEdit::Missing),
        Some(prior) if prior.origin != LOCAL_HIGHLIGHT_ORIGIN => {
            Err(HighlightEdit::ReadOnly(prior.origin))
        }
        Some(prior) => Ok(prior),
    })
}

fn marginalia_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ReadingMarginaliaRow, sqlx::Error> {
    Ok(ReadingMarginaliaRow {
        message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
        idx: row.try_get("idx")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        article_id: row.try_get("article_id")?,
        read_url: row.try_get("read_url")?,
        view: row.try_get("view")?,
        sent_at: decode_timestamp(row.try_get("sent_at")?)?,
        highlights_synced_at: decode_optional_timestamp(row.try_get("highlights_synced_at")?)?,
    })
}

impl super::Store {
    /// Every cached item of these messages, by message then index.
    pub async fn reading_items_for_messages(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<ReadingItemRow>, sqlx::Error> {
        let mut out = Vec::new();
        for chunk in message_ids.chunks(500) {
            let sql = format!(
                "SELECT * FROM reading_items WHERE message_id IN ({}) ORDER BY message_id, position, idx",
                placeholders(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push(item_from_row(&row)?);
            }
        }
        Ok(out)
    }

    /// Replace one message's cached items in a transaction.
    pub async fn replace_reading_items(
        &self,
        message_id: &MessageId,
        rows: &[ReadingItemRow],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        sqlx::query("DELETE FROM reading_items WHERE message_id = ?")
            .bind(message_id.as_str())
            .execute(&mut *tx)
            .await?;
        for row in rows {
            sqlx::query(
                "INSERT INTO reading_items
                     (message_id, idx, account_id, kind, shape, title, standfirst, url, domain,
                      tracked, words, extractor_version, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .bind(row.message_id.as_str())
            .bind(row.idx)
            .bind(row.account_id.as_str())
            .bind(&row.kind)
            .bind(&row.shape)
            .bind(&row.title)
            .bind(&row.standfirst)
            .bind(&row.url)
            .bind(&row.domain)
            .bind(i64::from(row.tracked))
            .bind(i64::from(row.words))
            .bind(i64::from(row.extractor_version))
            .bind(row.position)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// What you did with these messages' items.
    pub async fn reading_states_for_messages(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<ReadingStateRow>, sqlx::Error> {
        let mut out = Vec::new();
        for chunk in message_ids.chunks(500) {
            let sql = format!(
                "SELECT * FROM reading_state WHERE message_id IN ({})",
                placeholders(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push(state_from_row(&row)?);
            }
        }
        Ok(out)
    }

    /// Everything on Later in these accounts, newest first.
    pub async fn reading_later(
        &self,
        accounts: &[AccountId],
    ) -> Result<Vec<ReadingStateRow>, sqlx::Error> {
        if accounts.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT * FROM reading_state
             WHERE later_at IS NOT NULL AND account_id IN ({})
             ORDER BY later_at DESC, message_id, idx",
            placeholders(accounts.len())
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for account in accounts {
            query = query.bind(account.as_str());
        }
        query
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(state_from_row)
            .collect()
    }

    /// Put an item on Later or take it off. On Later already, `later`
    /// answers "Still want it?" with keep. Returns whether anything changed.
    pub async fn set_reading_later(
        &self,
        account_id: &AccountId,
        message_id: &MessageId,
        idx: i64,
        later: bool,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let at = now.timestamp();
        let result = if later {
            sqlx::query(
                "INSERT INTO reading_state (message_id, idx, account_id, later_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(message_id, idx) DO UPDATE SET
                     kept_at = CASE WHEN reading_state.later_at IS NOT NULL THEN ?4
                                    ELSE reading_state.kept_at END,
                     later_at = COALESCE(reading_state.later_at, ?4),
                     updated_at = ?4",
            )
            .bind(message_id.as_str())
            .bind(idx)
            .bind(account_id.as_str())
            .bind(at)
            .execute(self.writer())
            .await?
        } else {
            sqlx::query(
                "UPDATE reading_state SET later_at = NULL, kept_at = NULL, updated_at = ?3
                 WHERE message_id = ?1 AND idx = ?2 AND later_at IS NOT NULL",
            )
            .bind(message_id.as_str())
            .bind(idx)
            .bind(at)
            .execute(self.writer())
            .await?
        };
        Ok(result.rows_affected() > 0)
    }

    /// Add engagement: first opened, more time read, further down. Read to
    /// 90% or more is finished. Returns whether it is finished now.
    pub async fn record_reading_engagement(
        &self,
        account_id: &AccountId,
        message_id: &MessageId,
        idx: i64,
        report: ReadingEngagementReport,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let at = now.timestamp();
        let progress = if report.progress.is_finite() {
            report.progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dwell = i64::try_from(report.dwell_ms).unwrap_or(i64::MAX);
        let opened_at = report.opened.then_some(at);
        sqlx::query(
            "INSERT INTO reading_state
                 (message_id, idx, account_id, opened_at, dwell_ms, progress, finished_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, CASE WHEN ?6 >= 0.9 THEN ?7 END, ?7)
             ON CONFLICT(message_id, idx) DO UPDATE SET
                 opened_at = COALESCE(reading_state.opened_at, excluded.opened_at),
                 dwell_ms = reading_state.dwell_ms + excluded.dwell_ms,
                 progress = MAX(reading_state.progress, excluded.progress),
                 finished_at = COALESCE(reading_state.finished_at, excluded.finished_at),
                 updated_at = excluded.updated_at",
        )
        .bind(message_id.as_str())
        .bind(idx)
        .bind(account_id.as_str())
        .bind(opened_at)
        .bind(dwell)
        .bind(progress)
        .bind(at)
        .execute(self.writer())
        .await?;
        let finished: Option<i64> = sqlx::query_scalar(
            "SELECT finished_at FROM reading_state WHERE message_id = ? AND idx = ?",
        )
        .bind(message_id.as_str())
        .bind(idx)
        .fetch_optional(self.reader())
        .await?
        .flatten();
        Ok(finished.is_some())
    }

    /// Each sender's latest `per_sender` stored issues, newest first, with
    /// whether you opened or finished the issue in mxr.
    pub async fn reading_source_issues(
        &self,
        account_id: &AccountId,
        senders: &[String],
        per_sender: u32,
    ) -> Result<Vec<SourceIssue>, sqlx::Error> {
        let mut out = Vec::new();
        for chunk in senders.chunks(400) {
            let sql = format!(
                "SELECT ranked.from_email, ranked.id, ranked.date,
                        rs.opened_at IS NOT NULL AS opened,
                        rs.finished_at IS NOT NULL AS finished
                 FROM (
                     SELECT m.from_email, m.id, m.date,
                            ROW_NUMBER() OVER (PARTITION BY m.from_email ORDER BY m.date DESC) AS rn
                     FROM messages m
                     WHERE m.account_id = ?1 AND m.from_email IN ({})
                 ) ranked
                 LEFT JOIN reading_state rs ON rs.message_id = ranked.id AND rs.idx = 0
                 WHERE ranked.rn <= ?2
                 ORDER BY ranked.from_email, ranked.date DESC",
                numbered(3, chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(account_id.as_str())
                .bind(i64::from(per_sender));
            for sender in chunk {
                query = query.bind(sender);
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push(SourceIssue {
                    from_email: row.try_get("from_email")?,
                    message_id: decode_id(&row.try_get::<String, _>("id")?)?,
                    date: decode_timestamp(row.try_get("date")?)?,
                    opened: row.try_get::<i64, _>("opened")? != 0,
                    finished: row.try_get::<i64, _>("finished")? != 0,
                });
            }
        }
        Ok(out)
    }

    /// How many issues each sender has stored, all time.
    pub async fn reading_source_totals(
        &self,
        account_id: &AccountId,
        senders: &[String],
    ) -> Result<HashMap<String, u32>, sqlx::Error> {
        let mut out = HashMap::new();
        for chunk in senders.chunks(400) {
            let sql = format!(
                "SELECT from_email, COUNT(*) AS n FROM messages
                 WHERE account_id = ?1 AND from_email IN ({}) GROUP BY from_email",
                numbered(2, chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(account_id.as_str());
            for sender in chunk {
                query = query.bind(sender);
            }
            for row in query.fetch_all(self.reader()).await? {
                out.insert(
                    row.try_get::<String, _>("from_email")?,
                    u32::try_from(row.try_get::<i64, _>("n")?).unwrap_or(u32::MAX),
                );
            }
        }
        Ok(out)
    }

    /// Words and time read of items you finished, newest first, for your
    /// pace: the article's words when you read the article.
    pub async fn reading_finished_samples(
        &self,
        limit: u32,
    ) -> Result<Vec<(u32, u64)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT COALESCE(NULLIF(ra.words, 0), ri.words, 0) AS words, rs.dwell_ms
             FROM reading_state rs
             LEFT JOIN reading_items ri ON ri.message_id = rs.message_id AND ri.idx = rs.idx
             LEFT JOIN reading_articles ra ON ra.message_id = rs.message_id AND ra.idx = rs.idx
                 AND ra.status = 'ok'
             WHERE rs.finished_at IS NOT NULL AND rs.dwell_ms > 0
             ORDER BY rs.finished_at DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    u32::try_from(row.try_get::<i64, _>("words")?).unwrap_or(0),
                    u64::try_from(row.try_get::<i64, _>("dwell_ms")?).unwrap_or(0),
                ))
            })
            .collect()
    }

    pub async fn reading_article(
        &self,
        message_id: &MessageId,
        idx: i64,
    ) -> Result<Option<ReadingArticleRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM reading_articles WHERE message_id = ? AND idx = ?")
            .bind(message_id.as_str())
            .bind(idx)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(article_from_row)
            .transpose()
    }

    /// The items among these messages with a saved article.
    pub async fn reading_articles_saved(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<(MessageId, i64)>, sqlx::Error> {
        let mut out = Vec::new();
        for chunk in message_ids.chunks(500) {
            let sql = format!(
                "SELECT message_id, idx FROM reading_articles
                 WHERE status = 'ok' AND message_id IN ({})",
                placeholders(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push((
                    decode_id(&row.try_get::<String, _>("message_id")?)?,
                    row.try_get("idx")?,
                ));
            }
        }
        Ok(out)
    }

    pub async fn save_reading_article(&self, row: &ReadingArticleRow) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO reading_articles
                 (message_id, idx, url, final_url, status, title, byline, site_name, html,
                  paragraphs, words, contacted, error, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(message_id, idx) DO UPDATE SET
                 url = excluded.url, final_url = excluded.final_url, status = excluded.status,
                 title = excluded.title, byline = excluded.byline,
                 site_name = excluded.site_name, html = excluded.html,
                 paragraphs = excluded.paragraphs, words = excluded.words,
                 contacted = excluded.contacted, error = excluded.error,
                 fetched_at = excluded.fetched_at",
        )
        .bind(row.message_id.as_str())
        .bind(row.idx)
        .bind(&row.url)
        .bind(&row.final_url)
        .bind(&row.status)
        .bind(&row.title)
        .bind(&row.byline)
        .bind(&row.site_name)
        .bind(&row.html)
        .bind(&row.paragraphs)
        .bind(i64::from(row.words))
        .bind(&row.contacted)
        .bind(&row.error)
        .bind(row.fetched_at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn insert_reading_highlight(
        &self,
        row: &ReadingHighlightRow,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO reading_highlights
                 (id, account_id, message_id, idx, view, quote, note, created_at, origin, origin_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&row.id)
        .bind(row.account_id.as_str())
        .bind(row.message_id.as_str())
        .bind(row.idx)
        .bind(&row.view)
        .bind(&row.quote)
        .bind(&row.note)
        .bind(row.created_at.timestamp())
        .bind(&row.origin)
        .bind(&row.origin_id)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Highlights, oldest first: one account's, or every account's.
    pub async fn reading_highlights(
        &self,
        account_id: Option<&AccountId>,
    ) -> Result<Vec<ReadingHighlightRow>, sqlx::Error> {
        let rows = match account_id {
            Some(account) => {
                sqlx::query(
                    "SELECT * FROM reading_highlights WHERE account_id = ?
                     ORDER BY created_at, id",
                )
                .bind(account.as_str())
                .fetch_all(self.reader())
                .await?
            }
            None => {
                sqlx::query("SELECT * FROM reading_highlights ORDER BY created_at, id")
                    .fetch_all(self.reader())
                    .await?
            }
        };
        rows.iter().map(highlight_from_row).collect()
    }

    /// One highlight by id.
    pub async fn reading_highlight(
        &self,
        id: &str,
    ) -> Result<Option<ReadingHighlightRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM reading_highlights WHERE id = ?")
            .bind(id)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(highlight_from_row)
            .transpose()
    }

    /// Sets (or with `None` clears) a highlight's note. The read, the
    /// origin check and the write share one transaction on the writer, and
    /// the UPDATE repeats the origin test, so a read-back mirroring the
    /// same item can't interleave with it.
    pub async fn set_reading_highlight_note(
        &self,
        id: &str,
        note: Option<&str>,
    ) -> Result<HighlightEdit, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let prior = match own_highlight_in(&mut tx, id).await? {
            Ok(prior) => prior,
            Err(refusal) => return Ok(refusal),
        };
        let changed =
            sqlx::query("UPDATE reading_highlights SET note = ?1 WHERE id = ?2 AND origin = ?3")
                .bind(note)
                .bind(id)
                .bind(LOCAL_HIGHLIGHT_ORIGIN)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        tx.commit().await?;
        Ok(if changed == 0 {
            HighlightEdit::Missing
        } else {
            HighlightEdit::Done(prior)
        })
    }

    /// Removes a highlight, with the same guarantees as
    /// `set_reading_highlight_note`.
    pub async fn remove_reading_highlight(&self, id: &str) -> Result<HighlightEdit, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let prior = match own_highlight_in(&mut tx, id).await? {
            Ok(prior) => prior,
            Err(refusal) => return Ok(refusal),
        };
        let removed = sqlx::query("DELETE FROM reading_highlights WHERE id = ?1 AND origin = ?2")
            .bind(id)
            .bind(LOCAL_HIGHLIGHT_ORIGIN)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(if removed == 0 {
            HighlightEdit::Missing
        } else {
            HighlightEdit::Done(prior)
        })
    }

    /// Undo for the two edits above. A removed highlight is inserted again
    /// as it was (skipped if its message has since been deleted, and left
    /// alone if something already has its id); an edited one gets its old
    /// note back, only while it still exists. Returns how many are back.
    pub async fn put_back_reading_highlights(
        &self,
        priors: &[crate::HighlightPrior],
    ) -> Result<u32, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let mut back = 0;
        for prior in priors {
            let row = &prior.row;
            let affected = if prior.removed {
                sqlx::query(
                    "INSERT INTO reading_highlights
                         (id, account_id, message_id, idx, view, quote, note, created_at,
                          origin, origin_id)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
                     WHERE EXISTS (SELECT 1 FROM messages WHERE id = ?3)
                     ON CONFLICT(id) DO NOTHING",
                )
                .bind(&row.id)
                .bind(row.account_id.as_str())
                .bind(row.message_id.as_str())
                .bind(row.idx)
                .bind(&row.view)
                .bind(&row.quote)
                .bind(&row.note)
                .bind(row.created_at.timestamp())
                .bind(&row.origin)
                .bind(&row.origin_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
            } else {
                sqlx::query("UPDATE reading_highlights SET note = ?1 WHERE id = ?2 AND origin = ?3")
                    .bind(&row.note)
                    .bind(&row.id)
                    .bind(LOCAL_HIGHLIGHT_ORIGIN)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected()
            };
            if affected > 0 {
                back += 1;
            }
        }
        tx.commit().await?;
        Ok(back)
    }

    /// One message's highlights, oldest first.
    pub async fn reading_highlights_for_message(
        &self,
        message_id: &MessageId,
    ) -> Result<Vec<ReadingHighlightRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM reading_highlights WHERE message_id = ? ORDER BY created_at, id")
            .bind(message_id.as_str())
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(highlight_from_row)
            .collect()
    }

    /// The Marginalia article an item became, if it was sent.
    pub async fn reading_marginalia(
        &self,
        message_id: &MessageId,
        idx: i64,
    ) -> Result<Option<ReadingMarginaliaRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM reading_marginalia WHERE message_id = ? AND idx = ?")
            .bind(message_id.as_str())
            .bind(idx)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(marginalia_from_row)
            .transpose()
    }

    /// Items sent to Marginalia whose highlights were last read before
    /// `synced_before` (or never), sent since `sent_after`, least recently
    /// read first: one account's, or every account's.
    pub async fn reading_marginalia_due(
        &self,
        account_id: Option<&AccountId>,
        synced_before: DateTime<Utc>,
        sent_after: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<ReadingMarginaliaRow>, sqlx::Error> {
        sqlx::query(
            "SELECT * FROM reading_marginalia
             WHERE (?1 IS NULL OR account_id = ?1)
               AND (highlights_synced_at IS NULL OR highlights_synced_at < ?2)
               AND sent_at >= ?3
             ORDER BY COALESCE(highlights_synced_at, 0), sent_at
             LIMIT ?4",
        )
        .bind(account_id.map(AccountId::as_str))
        .bind(synced_before.timestamp())
        .bind(sent_after.timestamp())
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?
        .iter()
        .map(marginalia_from_row)
        .collect()
    }

    /// Records the article an item became, replacing an earlier one.
    pub async fn upsert_reading_marginalia(
        &self,
        row: &ReadingMarginaliaRow,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO reading_marginalia
                 (message_id, idx, account_id, article_id, read_url, view, sent_at,
                  highlights_synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (message_id, idx) DO UPDATE SET
                 article_id = excluded.article_id,
                 read_url = excluded.read_url,
                 view = excluded.view,
                 sent_at = excluded.sent_at,
                 highlights_synced_at = excluded.highlights_synced_at",
        )
        .bind(row.message_id.as_str())
        .bind(row.idx)
        .bind(row.account_id.as_str())
        .bind(&row.article_id)
        .bind(&row.read_url)
        .bind(&row.view)
        .bind(row.sent_at.timestamp())
        .bind(row.highlights_synced_at.map(|at| at.timestamp()))
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Forgets that an item was sent, when Marginalia no longer has the
    /// article. Highlights already read back stay.
    pub async fn delete_reading_marginalia(
        &self,
        message_id: &MessageId,
        idx: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM reading_marginalia WHERE message_id = ? AND idx = ?")
            .bind(message_id.as_str())
            .bind(idx)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Makes an item's Marginalia highlights match `highlights`: new ones
    /// are added, changed text is updated, and ones no longer there are
    /// removed. Highlights made in mxr are never touched.
    pub async fn mirror_marginalia_highlights(
        &self,
        sent: &ReadingMarginaliaRow,
        highlights: &[MarginaliaHighlightIn],
        synced_at: DateTime<Utc>,
    ) -> Result<MarginaliaMirror, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let existing: Vec<(String, String, String)> = sqlx::query(
            "SELECT id, origin_id, quote FROM reading_highlights
             WHERE message_id = ? AND idx = ? AND origin = 'marginalia'",
        )
        .bind(sent.message_id.as_str())
        .bind(sent.idx)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("id")?,
                row.try_get::<Option<String>, _>("origin_id")?
                    .unwrap_or_default(),
                row.try_get("quote")?,
            ))
        })
        .collect::<Result<_, sqlx::Error>>()?;
        let mut mirror = MarginaliaMirror::default();
        for (id, origin_id, _) in &existing {
            if !highlights.iter().any(|h| &h.origin_id == origin_id) {
                sqlx::query("DELETE FROM reading_highlights WHERE id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                mirror.removed += 1;
            }
        }
        for highlight in highlights {
            match existing
                .iter()
                .find(|(_, origin_id, _)| *origin_id == highlight.origin_id)
            {
                Some((id, _, quote)) if *quote != highlight.quote => {
                    sqlx::query("UPDATE reading_highlights SET quote = ? WHERE id = ?")
                        .bind(&highlight.quote)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                }
                Some(_) => {}
                None => {
                    sqlx::query(
                        "INSERT INTO reading_highlights
                             (id, account_id, message_id, idx, view, quote, note, created_at,
                              origin, origin_id)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, 'marginalia', ?8)",
                    )
                    .bind(uuid::Uuid::now_v7().to_string())
                    .bind(sent.account_id.as_str())
                    .bind(sent.message_id.as_str())
                    .bind(sent.idx)
                    .bind(&sent.view)
                    .bind(&highlight.quote)
                    .bind(highlight.created_at.timestamp())
                    .bind(&highlight.origin_id)
                    .execute(&mut *tx)
                    .await?;
                    mirror.added += 1;
                }
            }
        }
        sqlx::query(
            "UPDATE reading_marginalia SET highlights_synced_at = ?
             WHERE message_id = ? AND idx = ?",
        )
        .bind(synced_at.timestamp())
        .bind(sent.message_id.as_str())
        .bind(sent.idx)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(mirror)
    }

    /// Per-source choices for these senders, by lowercase address.
    pub async fn reading_source_prefs(
        &self,
        account_id: &AccountId,
    ) -> Result<HashMap<String, ReadingSourcePrefs>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT sender_email, original_layout, unsubscribe_offer_dismissed_at
             FROM reading_sources WHERE account_id = ?",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("sender_email")?,
                    ReadingSourcePrefs {
                        original_layout: row.try_get::<i64, _>("original_layout")? != 0,
                        unsubscribe_offer_dismissed: row
                            .try_get::<Option<i64>, _>("unsubscribe_offer_dismissed_at")?
                            .is_some(),
                    },
                ))
            })
            .collect()
    }

    pub async fn set_reading_source_prefs(
        &self,
        account_id: &AccountId,
        sender_email: &str,
        original_layout: Option<bool>,
        dismiss_unsubscribe_offer: bool,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO reading_sources
                 (account_id, sender_email, original_layout, unsubscribe_offer_dismissed_at)
             VALUES (?1, ?2, COALESCE(?3, 0), CASE WHEN ?4 THEN ?5 END)
             ON CONFLICT(account_id, sender_email) DO UPDATE SET
                 original_layout = COALESCE(?3, reading_sources.original_layout),
                 unsubscribe_offer_dismissed_at = COALESCE(
                     reading_sources.unsubscribe_offer_dismissed_at,
                     CASE WHEN ?4 THEN ?5 END)",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .bind(original_layout.map(i64::from))
        .bind(dismiss_unsubscribe_offer)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// When Reading was opened in `account_id`.
    pub async fn reading_visit(
        &self,
        account_id: &AccountId,
    ) -> Result<ReadingVisitRow, sqlx::Error> {
        let row = sqlx::query(
            "SELECT first_seen, boundary, last_seen FROM reading_visit WHERE account_id = ?",
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        let Some(row) = row else {
            return Ok(ReadingVisitRow::default());
        };
        Ok(ReadingVisitRow {
            first_seen: decode_optional_timestamp(row.try_get("first_seen")?)?,
            boundary: decode_optional_timestamp(row.try_get("boundary")?)?,
            last_seen: decode_optional_timestamp(row.try_get("last_seen")?)?,
        })
    }

    pub async fn set_reading_visit(
        &self,
        account_id: &AccountId,
        visit: &ReadingVisitRow,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO reading_visit (account_id, first_seen, boundary, last_seen)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id) DO UPDATE SET
                 first_seen = COALESCE(reading_visit.first_seen, excluded.first_seen),
                 boundary = excluded.boundary,
                 last_seen = excluded.last_seen",
        )
        .bind(account_id.as_str())
        .bind(visit.first_seen.map(|at| at.timestamp()))
        .bind(visit.boundary.map(|at| at.timestamp()))
        .bind(visit.last_seen.map(|at| at.timestamp()))
        .execute(self.writer())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
