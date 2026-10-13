//! Offline bundles are built from allowed logical rows, never source database pages.
use crate::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::AssertSqlSafe;
use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub type BackupResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub path: String,
    pub length: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub schema_version: i64,
    pub workspace_id: String,
    pub recovery_generation: String,
    pub files: Vec<BackupFile>,
    pub exclusions: Vec<String>,
}

const TABLES: &[&str] = &[
    "accounts",
    "labels",
    "messages",
    "message_labels",
    "bodies",
    "attachments",
    "provider_meta",
    "drafts",
    "snoozed",
    "saved_searches",
    "rules",
    "rule_execution_log",
    "semantic_profiles",
    "message_events",
    "account_addresses",
    "reply_pairs",
    "reply_pair_pending",
    "contacts",
    "mutation_undo_log",
    "message_flags",
    "auto_reminders",
    "snippets",
    "screener_decisions",
    "signatures",
    "signature_defaults",
    "thread_summaries",
    "contact_style",
    "contact_relationship_summary",
    "contact_commitments",
    "user_voice_profile",
    "draft_safety_runs",
    "draft_commitment_candidates",
    "decision_log",
    "relationship_watchlist",
    "context_briefings",
    "decision_log_v2",
    "calendar_invites",
    "message_keywords",
    "deliveries",
    "delivery_messages",
    "sent_draft_receipts",
    "mutation_jobs",
    "scheduled_send_attempts",
    "desk_dismissals",
    "message_pins",
    "history_text_fingerprints",
    "decision_evidence",
    "todos",
    "todo_runs",
    "mode_views",
    "mode_guide_seen",
    "mode_done",
    "person_links",
    "got_it_sends",
    "update_facts",
    "update_sources",
    "reading_items",
    "reading_state",
    "reading_articles",
    "reading_highlights",
    "reading_sources",
    "reading_visit",
    "record_groups",
    "records",
    "record_messages",
    "record_fields",
    "record_senders",
    "record_runs",
    "arrivals",
    "initial_syncs",
    "mode_corrections",
    "rule_treatments",
    "draft_send_outcomes",
    "archive_workspace",
    "record_renewal_cycles",
    "personal_categories",
    "message_category_memberships",
    "category_membership_undo",
    "todo_resolution_suggestions",
    "todo_resolution_evidence",
    "draft_attachment_resources",
    "rule_category_sources",
    "restore_recovery",
    "restored_draft_holds",
    "restored_intentions",
    "todo_resolution_queue",
];

pub fn checked_relative(value: &str) -> BackupResult<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Bundle paths must be nonempty relative paths without traversal".into());
    }
    Ok(path.to_path_buf())
}

fn plain_file(path: &Path) -> BackupResult<()> {
    if !path
        .symlink_metadata()
        .map_err(|e| format!("Inspecting file {}: {e}", path.display()))?
        .is_file()
    {
        return Err(format!(
            "Required backup file is missing, symlinked or not regular: {}",
            path.display()
        )
        .into());
    }
    let mut parent = path.parent();
    while let Some(dir) = parent {
        if dir
            .symlink_metadata()
            .map_err(|e| format!("Inspecting parent {}: {e}", dir.display()))?
            .file_type()
            .is_symlink()
        {
            return Err("Backup resource parent must not be a symlink".into());
        }
        parent = dir.parent();
    }
    Ok(())
}

pub(crate) fn file_digest(path: &Path) -> BackupResult<(u64, String)> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut hash = Sha256::new();
    let mut length = 0_u64;
    loop {
        let read = match file.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        length += read as u64;
    }
    Ok((
        length,
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}

pub fn file_metadata(root: &Path, relative: &str) -> BackupResult<BackupFile> {
    let path = root.canonicalize()?.join(checked_relative(relative)?);
    plain_file(&path)?;
    let (length, sha256) = file_digest(&path)?;
    Ok(BackupFile {
        path: relative.to_owned(),
        length,
        sha256,
    })
}

pub async fn inspect(bundle: &Path) -> BackupResult<BackupManifest> {
    let bundle_path = bundle.canonicalize()?;
    let bundle = bundle_path.as_path();
    plain_file(&bundle.join("manifest.json"))?;
    let manifest: BackupManifest =
        serde_json::from_slice(&std::fs::read(bundle.join("manifest.json"))?)?;
    if manifest.format_version != 1 || !(81..=84).contains(&manifest.schema_version) {
        return Err("Unsupported backup format or schema".into());
    }
    let mut names = BTreeSet::new();
    for expected in &manifest.files {
        if expected.path != "mxr.db"
            && expected.path != "settings.json"
            && !expected.path.starts_with("resources/cache/")
            && !expected.path.starts_with("mxr.draft-attachments/")
        {
            return Err("Unexpected backup payload path".into());
        }
        if !names.insert(expected.path.clone()) {
            return Err("Duplicate bundle path".into());
        }
        let actual = file_metadata(bundle, &expected.path)?;
        if actual.length != expected.length || actual.sha256 != expected.sha256 {
            return Err(format!("Backup file checksum mismatch: {}", expected.path).into());
        }
    }
    fn inventory(root: &Path, dir: &Path, names: &mut BTreeSet<String>) -> BackupResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            let meta = path.symlink_metadata()?;
            if meta.file_type().is_symlink() {
                return Err("Bundle must not contain symlinks".into());
            }
            if meta.is_dir() {
                inventory(root, &path, names)?;
            } else if meta.is_file() {
                names.insert(path.strip_prefix(root)?.to_string_lossy().into_owned());
            } else {
                return Err("Bundle contains a nonregular file".into());
            }
        }
        Ok(())
    }
    let mut actual_names = BTreeSet::new();
    inventory(bundle, bundle, &mut actual_names)?;
    actual_names.remove("manifest.json");
    if names != actual_names {
        return Err("Bundle file inventory differs from its manifest".into());
    }
    if !names.contains("settings.json") {
        return Err("Backup manifest has no sanitized settings".into());
    }
    if !names.contains("mxr.db") {
        return Err("Backup manifest has no database".into());
    }
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(bundle.join("mxr.db"))
            .read_only(true),
    )
    .await?;
    validate(&mut db).await?;
    let expected = Store::in_memory().await?;
    if manifest.schema_version < 84 {
        sqlx::query("DROP TABLE remote_pairing_retirements")
            .execute(expected.writer())
            .await?;
    }
    if manifest.schema_version < 83 {
        sqlx::raw_sql(
            "DROP TABLE remote_clients; DROP TABLE remote_pairings; DROP TABLE remote_host;",
        )
        .execute(expected.writer())
        .await?;
    }
    if manifest.schema_version == 81 {
        // Compare legacy bundles against their immutable schema, not today's migrated schema.
        sqlx::query("DROP TABLE todo_resolution_queue")
            .execute(expected.writer())
            .await?;
    }
    let schema_sql="SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name";
    let actual_schema: Vec<(String, String, String, Option<String>)> =
        sqlx::query_as(schema_sql).fetch_all(&mut db).await?;
    let expected_schema: Vec<(String, String, String, Option<String>)> = sqlx::query_as(schema_sql)
        .fetch_all(expected.reader())
        .await?;
    expected.writer().close().await;
    if actual_schema != expected_schema {
        return Err("Unsupported backup schema definition".into());
    }
    if manifest.schema_version >= 83 {
        for table in ["remote_clients", "remote_pairings", "remote_host"] {
            let count: i64 =
                sqlx::query_scalar(AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&mut db)
                    .await?;
            if count != 0 {
                return Err("Portable backup contains remote authority".into());
            }
        }
    }
    if manifest.schema_version >= 84 {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_pairing_retirements")
            .fetch_one(&mut db)
            .await?;
        if count != 0 {
            return Err("Portable backup contains remote pairing retirements".into());
        }
    }
    super::backup_resources::verify_references(&mut db, &manifest.files).await?;
    let schema: i64 = sqlx::query_scalar("SELECT MAX(version) FROM schema_migrations")
        .fetch_one(&mut db)
        .await?;
    if schema != manifest.schema_version {
        return Err("Backup schema stamp mismatch".into());
    }
    for excluded in [
        "user_activity",
        "saved_activity_filters",
        "draft_safety_overrides",
    ] {
        let count: i64 =
            sqlx::query_scalar(AssertSqlSafe(format!("SELECT COUNT(*) FROM {excluded}")))
                .fetch_one(&mut db)
                .await?;
        if count != 0 {
            return Err("Backup contains excluded private operational state".into());
        }
    }
    let identity: (String, String) = sqlx::query_as(
        "SELECT workspace_id,recovery_generation FROM archive_workspace WHERE singleton=1",
    )
    .fetch_one(&mut db)
    .await?;
    if identity
        != (
            manifest.workspace_id.clone(),
            manifest.recovery_generation.clone(),
        )
    {
        return Err("Backup workspace identity mismatch".into());
    }
    db.close().await?;
    Ok(manifest)
}

async fn validate(db: &mut SqliteConnection) -> BackupResult<()> {
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut *db)
        .await?;
    if integrity != "ok" {
        return Err("Backup database integrity check failed".into());
    }
    if !sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut *db)
        .await?
        .is_empty()
    {
        return Err("Backup database foreign key check failed".into());
    }
    Ok(())
}

/// Caller holds the source's exclusive lease until publication completes.
pub async fn populate(source: &Path, stage: &Path) -> BackupResult<BackupManifest> {
    let source = source.canonicalize()?;
    let source = source.as_path();
    let stage_path = stage.canonicalize()?;
    let stage = stage_path.as_path();
    let scratch = tempfile::tempdir()?;
    let source_db = source.join("mxr.db");
    plain_file(&source_db).map_err(|e| format!("Source database validation: {e}"))?;
    for suffix in ["", "-wal", "-shm"] {
        let from = source.join(format!("mxr.db{suffix}"));
        if from.exists() {
            plain_file(&from)?;
            std::fs::copy(from, scratch.path().join(format!("source.db{suffix}")))?;
        }
    }
    let store = Store::fresh_backup(&stage.join("mxr.db"))
        .await
        .map_err(|e| format!("Fresh backup schema: {e}"))?;
    let mut connection = store.writer().acquire().await?;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await?;
    sqlx::query("ATTACH DATABASE ? AS source")
        .bind(scratch.path().join("source.db").to_string_lossy().as_ref())
        .execute(&mut *connection)
        .await?;
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM source.schema_migrations")
        .fetch_one(&mut *connection)
        .await?;
    if !(80..=84).contains(&version) {
        return Err("Unsupported source schema; upgrade before backup".into());
    }
    for required in [
        "accounts",
        "messages",
        "drafts",
        "draft_attachment_resources",
        "archive_workspace",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source.sqlite_schema WHERE type='table' AND name=?)",
        )
        .bind(required)
        .fetch_one(&mut *connection)
        .await?;
        if !exists {
            return Err(format!("Incomplete source schema: missing {required}").into());
        }
    }
    for table in TABLES {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source.sqlite_schema WHERE type='table' AND name=?)",
        )
        .bind(table)
        .fetch_one(&mut *connection)
        .await?;
        if !exists {
            continue;
        }
        // Identifiers come only from this allowlist and the freshly migrated main schema.
        let columns = sqlx::query(AssertSqlSafe(format!(
            "PRAGMA main.table_info(\"{table}\")"
        )))
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|r| format!("\"{}\"", r.get::<String, _>("name")))
        .collect::<Vec<_>>()
        .join(",");
        sqlx::query(AssertSqlSafe(format!("DELETE FROM main.\"{table}\"")))
            .execute(&mut *connection)
            .await?;
        let schema: String = sqlx::query_scalar("SELECT sql FROM main.sqlite_schema WHERE name=?")
            .bind(table)
            .fetch_one(&mut *connection)
            .await?;
        let rowid = if schema.to_ascii_uppercase().contains("WITHOUT ROWID") {
            ""
        } else {
            "rowid,"
        };
        sqlx::query(AssertSqlSafe(format!("INSERT INTO main.\"{table}\" ({rowid}{columns}) SELECT {rowid}{columns} FROM source.\"{table}\""))).execute(&mut *connection).await.map_err(|e|format!("Copying durable table {table}: {e}"))?;
    }
    sqlx::query("DETACH DATABASE source")
        .execute(&mut *connection)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await?;
    drop(connection);
    let (workspace_id, recovery_generation) = store.archive_workspace().await?;
    let mut files = super::backup_resources::copy_resources(&store, source, stage)
        .await
        .map_err(|e| format!("Referenced resource inventory: {e}"))?;
    let mut connection = store.writer().acquire().await?;
    validate(&mut connection).await?;
    drop(connection);
    store.writer().close().await;
    files
        .push(file_metadata(stage, "mxr.db").map_err(|e| format!("Sanitized database file: {e}"))?);
    Ok(BackupManifest { format_version:1, schema_version:84, workspace_id,recovery_generation,files,
        exclusions: vec!["activity and activity FTS/shadows/filters; safety override tokens".into(),"remote pairing/registration/host authority, credentials, auth files, logs, models, derived indexes, source WAL/SHM and runtime bookkeeping".into()] })
}

/// Retire runnable state before any restored Store or daemon can open it.
pub async fn retire_restored(db_path: &Path) -> BackupResult<()> {
    let mut db =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(db_path)).await?;
    let mut tx = db.begin().await?;
    let remote_tables: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='remote_host')")
            .fetch_one(&mut *tx)
            .await?;
    if remote_tables {
        let retirement_table: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='remote_pairing_retirements')",
        )
        .fetch_one(&mut *tx)
        .await?;
        let sql = if retirement_table {
            "DELETE FROM remote_pairing_retirements; DELETE FROM remote_clients; DELETE FROM remote_pairings; DELETE FROM remote_host;"
        } else {
            "DELETE FROM remote_clients; DELETE FROM remote_pairings; DELETE FROM remote_host;"
        };
        sqlx::raw_sql(sql).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO restore_recovery VALUES(1,1,0) ON CONFLICT(singleton) DO UPDATE SET pending=1,source_owner_stopped=0").execute(&mut *tx).await?;
    sqlx::query("UPDATE archive_workspace SET recovery_generation=? WHERE singleton=1")
        .bind(uuid::Uuid::now_v7().to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO restored_draft_holds SELECT id,account_id,revision,subject,NULL FROM drafts WHERE 1 ON CONFLICT(draft_id) DO NOTHING").execute(&mut *tx).await?;
    for (table, key, kind, account, state, selection) in [
        (
            "drafts",
            "id",
            "scheduled-send",
            "account_id",
            "'scheduled'",
            "send_at IS NOT NULL",
        ),
        (
            "mutation_jobs",
            "job_id",
            "mutation-job",
            "NULL",
            "status",
            "1",
        ),
        (
            "snoozed",
            "message_id",
            "snooze",
            "account_id",
            "'pending'",
            "1",
        ),
        (
            "auto_reminders",
            "sent_message_id",
            "auto-reminder",
            "account_id",
            "'pending'",
            "triggered_at IS NULL AND cancelled_at IS NULL",
        ),
        (
            "drafts",
            "id",
            "provider-draft-link",
            "account_id",
            "'detached'",
            "provider_draft_id IS NOT NULL",
        ),
        (
            "drafts",
            "id",
            "draft-send-state",
            "account_id",
            "status",
            "status != 'draft'",
        ),
        ("rules", "id", "rule", "NULL", "'enabled'", "enabled=1"),
        (
            "message_flags",
            "message_id",
            "reply-later-timer",
            "NULL",
            "'pending'",
            "reply_later_due_at IS NOT NULL AND reply_later_returned_at IS NULL",
        ),
    ] {
        let rows = sqlx::query(AssertSqlSafe(format!("SELECT {key} AS original_id,{account} AS account_id,{state} AS original_state FROM {table} WHERE {selection}"))).fetch_all(&mut *tx).await?;
        for row in rows {
            let id: String = row.get("original_id");
            // Preserve domain payload as SQL JSON, including the original scheduling identity.
            let columns = sqlx::query(AssertSqlSafe(format!("PRAGMA table_info({table})")))
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|r| r.get::<String, _>("name"))
                .collect::<Vec<_>>();
            let fields = columns
                .iter()
                .map(|c| format!("'{c}',\"{c}\""))
                .collect::<Vec<_>>()
                .join(",");
            let payload: String = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT json_object({fields}) FROM {table} WHERE {key}=?"
            )))
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
            let outcome_reference = if kind == "scheduled-send" {
                let schedule: serde_json::Value = serde_json::from_str(&payload)?;
                Some(format!(
                    "scheduled:{id}:{}:{}",
                    schedule["scheduled_send_revision"], schedule["send_at"]
                ))
            } else {
                None
            };
            sqlx::query("INSERT OR IGNORE INTO restored_intentions VALUES(?,?,?,?,?,?)")
                .bind(&id)
                .bind(kind)
                .bind(row.get::<Option<String>, _>("account_id"))
                .bind(row.get::<String, _>("original_state"))
                .bind(outcome_reference)
                .bind(payload)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query("INSERT OR IGNORE INTO restored_intentions SELECT draft_id,'previous-preparation',account_id,'retired',prepared_draft_id,json_object('prepared_draft_id',prepared_draft_id) FROM restored_draft_holds WHERE prepared_draft_id IS NOT NULL").execute(&mut *tx).await?;
    sqlx::raw_sql("UPDATE restored_draft_holds SET prepared_draft_id=NULL; UPDATE drafts SET send_at=NULL,scheduled_send_revision=NULL,provider_draft_id=NULL,provider_draft_revision=NULL,status='draft'; UPDATE rules SET enabled=0; UPDATE message_flags SET reply_later_due_at=NULL; DELETE FROM snoozed; UPDATE auto_reminders SET cancelled_at=unixepoch() WHERE triggered_at IS NULL AND cancelled_at IS NULL; DELETE FROM mutation_jobs; UPDATE accounts SET enabled=0;").execute(&mut *tx).await?;
    tx.commit().await?;
    validate(&mut db).await?;
    db.close().await?;
    Ok(())
}

pub use crate::backup_resources::remap_restored;

#[cfg(test)]
mod tests;
