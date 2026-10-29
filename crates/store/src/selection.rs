//! Store reads for resolving a bulk selection.

use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::Row;

impl super::Store {
    /// The messages among `ids` that are still stored, with their account and
    /// thread, in `ids` order.
    ///
    /// A search index can hold ids the store has since dropped. This is the
    /// existence check for them without loading envelopes: three columns per
    /// message instead of a full envelope, labels and keywords.
    pub async fn stored_message_homes(
        &self,
        ids: &[MessageId],
    ) -> Result<Vec<(MessageId, AccountId, ThreadId)>, sqlx::Error> {
        let mut homes = std::collections::HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT id, account_id, thread_id FROM messages WHERE id IN ({placeholders})"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                let id: MessageId = crate::decode_id(row.get::<String, _>("id").as_str())?;
                let account: AccountId =
                    crate::decode_id(row.get::<String, _>("account_id").as_str())?;
                let thread: ThreadId =
                    crate::decode_id(row.get::<String, _>("thread_id").as_str())?;
                homes.insert(id, (account, thread));
            }
        }
        Ok(ids
            .iter()
            .filter_map(|id| {
                homes
                    .remove(id)
                    .map(|(account, thread)| (id.clone(), account, thread))
            })
            .collect())
    }
}
