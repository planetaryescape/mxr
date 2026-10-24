//! Durable authority changes invalidate generations in the same transaction.
use crate::Store;
use std::collections::HashMap;

impl Store {
    pub fn normalized_receiver_services(ids: &[String]) -> Vec<String> {
        let mut ids: Vec<_> = ids.iter().map(|id| id.to_ascii_lowercase()).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    pub async fn sync_receiver_authority(
        &self,
        configured: &HashMap<String, Vec<String>>,
    ) -> Result<(), sqlx::Error> {
        let desired = configured
            .iter()
            .filter_map(|(account, ids)| {
                let ids = Self::normalized_receiver_services(ids);
                (!ids.is_empty())
                    .then(|| Ok((format!("receiver:{account}"), crate::encode_json(&ids)?)))
            })
            .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
        self.sync_authorities("receiver:", desired).await
    }

    pub async fn sync_assessment_context(&self, value: &str) -> Result<(), sqlx::Error> {
        self.sync_authorities("global", HashMap::from([("global".into(), value.into())]))
            .await
    }

    async fn sync_authorities(
        &self,
        prefix: &str,
        desired: HashMap<String, String>,
    ) -> Result<(), sqlx::Error> {
        const READ: &str = "SELECT key,value FROM assessment_authority WHERE key LIKE ?1 || '%'";
        let old: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(READ)
            .bind(prefix)
            .fetch_all(self.reader())
            .await?
            .into_iter()
            .collect();
        if old == desired {
            return Ok(());
        }
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let old: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(READ)
            .bind(prefix)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
        if old == desired {
            return tx.rollback().await;
        }
        let mut keys: Vec<_> = old.keys().chain(desired.keys()).cloned().collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            if old.get(&key) == desired.get(&key) {
                continue;
            }
            let account = (prefix != "global").then(|| &key[prefix.len()..]);
            sqlx::query("UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1")
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty) SELECT DISTINCT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id=1), 1 FROM messages WHERE (?1 IS NULL OR account_id=?1) ON CONFLICT(account_id, thread_id) DO UPDATE SET revision=excluded.revision, dirty=1")
                .bind(account).execute(&mut *tx).await?;
            if let Some(value) = desired.get(&key) {
                sqlx::query("INSERT INTO assessment_authority(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value").bind(&key).bind(value).execute(&mut *tx).await?;
            } else {
                sqlx::query("DELETE FROM assessment_authority WHERE key=?")
                    .bind(&key)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await
    }
}
