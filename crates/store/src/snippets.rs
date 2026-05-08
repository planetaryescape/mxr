//! Snippets: user-defined short-text expansions for compose.
//!
//! Local-only table keyed by a short keyword (e.g. `sig`, `thanks`).
//! Body is markdown. `vars` is a JSON array of declared placeholders
//! (e.g. `["first_name"]`) so the compose flow can warn at send-time
//! about any unfilled `{first_name}` left in the body.

use crate::{decode_json, decode_timestamp, encode_json, trace_lookup, trace_query};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub name: String,
    pub body: String,
    pub vars: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl super::Store {
    pub async fn upsert_snippet(&self, snippet: &Snippet) -> Result<(), sqlx::Error> {
        let vars = encode_json(&snippet.vars)?;
        let created = snippet.created_at.timestamp();
        let updated = snippet.updated_at.timestamp();
        sqlx::query!(
            r#"INSERT INTO snippets (name, body, vars, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(name) DO UPDATE SET
                   body = excluded.body,
                   vars = excluded.vars,
                   updated_at = excluded.updated_at"#,
            snippet.name,
            snippet.body,
            vars,
            created,
            updated,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn delete_snippet(&self, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM snippets WHERE name = ?", name)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_snippet(&self, name: &str) -> Result<Option<Snippet>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT name as "name!", body as "body!", vars as "vars!",
                      created_at as "created_at!", updated_at as "updated_at!"
               FROM snippets WHERE name = ?"#,
            name,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("snippets.get", started_at, row.is_some());
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(Snippet {
                name: r.name,
                body: r.body,
                vars: decode_json(&r.vars)?,
                created_at: decode_timestamp(r.created_at)?,
                updated_at: decode_timestamp(r.updated_at)?,
            })),
        }
    }

    pub async fn list_snippets(&self) -> Result<Vec<Snippet>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT name as "name!", body as "body!", vars as "vars!",
                      created_at as "created_at!", updated_at as "updated_at!"
               FROM snippets ORDER BY name ASC"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("snippets.list", started_at, rows.len());
        rows.into_iter()
            .map(|r| {
                Ok(Snippet {
                    name: r.name,
                    body: r.body,
                    vars: decode_json(&r.vars)?,
                    created_at: decode_timestamp(r.created_at)?,
                    updated_at: decode_timestamp(r.updated_at)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::Store;
    use super::Snippet;
    use chrono::{TimeZone, Utc};

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    fn make(name: &str, body: &str, vars: &[&str]) -> Snippet {
        Snippet {
            name: name.into(),
            body: body.into(),
            vars: vars.iter().map(|s| s.to_string()).collect(),
            created_at: anchor(),
            updated_at: anchor(),
        }
    }

    #[tokio::test]
    async fn upsert_and_get_round_trips() {
        let store = Store::in_memory().await.unwrap();
        let s = make("sig", "-- \nAlice", &[]);
        store.upsert_snippet(&s).await.unwrap();
        let got = store.get_snippet("sig").await.unwrap().unwrap();
        assert_eq!(got, s);
    }

    #[tokio::test]
    async fn upsert_replaces_existing_body_and_vars() {
        let store = Store::in_memory().await.unwrap();
        store
            .upsert_snippet(&make("thanks", "old", &["a"]))
            .await
            .unwrap();
        store
            .upsert_snippet(&make("thanks", "new", &["b", "c"]))
            .await
            .unwrap();
        let got = store.get_snippet("thanks").await.unwrap().unwrap();
        assert_eq!(got.body, "new");
        assert_eq!(got.vars, vec!["b", "c"]);
    }

    #[tokio::test]
    async fn delete_removes_existing_snippet() {
        let store = Store::in_memory().await.unwrap();
        store
            .upsert_snippet(&make("sig", "Alice", &[]))
            .await
            .unwrap();
        let removed = store.delete_snippet("sig").await.unwrap();
        assert!(removed);
        assert!(store.get_snippet("sig").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_returns_false_for_missing_snippet() {
        let store = Store::in_memory().await.unwrap();
        assert!(!store.delete_snippet("nope").await.unwrap());
    }

    #[tokio::test]
    async fn list_returns_alphabetical_order() {
        let store = Store::in_memory().await.unwrap();
        for name in ["sig", "thanks", "intro"] {
            store
                .upsert_snippet(&make(name, "body", &[]))
                .await
                .unwrap();
        }
        let names: Vec<_> = store
            .list_snippets()
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["intro", "sig", "thanks"]);
    }
}
