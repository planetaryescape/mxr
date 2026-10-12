use crate::backup::{checked_relative, file_metadata, BackupFile, BackupResult};
use crate::Store;
use mxr_core::DraftAttachment;
use serde_json::Value;
use sqlx::AssertSqlSafe;
use sqlx::{Connection, Row, SqliteConnection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn portable_file(
    source: &Path,
    stage: &Path,
    original: &Path,
    copied: &mut HashMap<PathBuf, String>,
    files: &mut Vec<BackupFile>,
) -> BackupResult<String> {
    if let Some(existing) = copied.get(original) {
        return Ok(existing.clone());
    }
    if original.symlink_metadata()?.file_type().is_symlink() {
        return Err("Backup resources must not be symlinks".into());
    }
    let mut parent = original.parent();
    while let Some(dir) = parent {
        if dir.canonicalize()? == source.canonicalize()? {
            break;
        }
        if dir.symlink_metadata()?.file_type().is_symlink() {
            return Err("Backup resource parents must not be symlinks".into());
        }
        parent = dir.parent();
    }
    let canonical = original.canonicalize().map_err(|e| {
        format!(
            "Incomplete backup: required file {}: {e}",
            original.display()
        )
    })?;
    if !canonical.starts_with(source.canonicalize()?) {
        return Err(format!("Incomplete backup: unsupported external legacy attachment {}; import it as a managed draft resource first",original.display()).into());
    }
    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid resource filename")?;
    checked_relative(name)?;
    let digest = sha2::Sha256::digest(std::fs::read(&canonical)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let relative = format!("resources/cache/{digest}/{name}");
    let target = stage.join(&relative);
    std::fs::create_dir_all(target.parent().ok_or("Resource has no parent")?)?;
    std::fs::copy(&canonical, &target)?;
    let metadata = file_metadata(stage, &relative)?;
    if !files.iter().any(|f| f.path == relative) {
        files.push(metadata);
    }
    copied.insert(original.to_path_buf(), relative.clone());
    Ok(relative)
}
use sha2::Digest;

pub(super) async fn copy_resources(
    store: &Store,
    source: &Path,
    stage: &Path,
) -> BackupResult<Vec<BackupFile>> {
    let mut files = Vec::new();
    let mut copied = HashMap::new();
    for row in sqlx::query("SELECT id,local_path FROM attachments WHERE local_path IS NOT NULL")
        .fetch_all(store.reader())
        .await?
    {
        let original: String = row.get("local_path");
        let relative = portable_file(source, stage, Path::new(&original), &mut copied, &mut files)?;
        sqlx::query("UPDATE attachments SET local_path=? WHERE id=?")
            .bind(relative)
            .bind(row.get::<String, _>("id"))
            .execute(store.writer())
            .await?;
    }
    for (table, key, column, objects) in [
        ("drafts", "id", "attachments", false),
        ("drafts", "id", "inline_assets", true),
    ] {
        for row in sqlx::query(AssertSqlSafe(format!("SELECT {key},{column} FROM {table}")))
            .fetch_all(store.reader())
            .await?
        {
            let mut value: Value = serde_json::from_str(&row.get::<String, _>(column))?;
            if let Some(items) = value.as_array_mut() {
                for item in items {
                    let path = if objects {
                        item.get(if column == "attachments" {
                            "local_path"
                        } else {
                            "path"
                        })
                        .and_then(Value::as_str)
                    } else {
                        item.as_str()
                    };
                    if let Some(path) = path {
                        let relative =
                            portable_file(source, stage, Path::new(path), &mut copied, &mut files)?;
                        if objects {
                            item[if column == "attachments" {
                                "local_path"
                            } else {
                                "path"
                            }] = Value::String(relative);
                        } else {
                            *item = Value::String(relative);
                        }
                    }
                }
            }
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE {table} SET {column}=? WHERE {key}=?"
            )))
            .bind(serde_json::to_string(&value)?)
            .bind(row.get::<String, _>(key))
            .execute(store.writer())
            .await?;
        }
    }
    for row in sqlx::query("SELECT metadata_json,ready FROM draft_attachment_resources")
        .fetch_all(store.reader())
        .await?
    {
        if !row.get::<bool, _>("ready") {
            return Err(
                "Incomplete backup: unfinished draft attachment upload; finish or remove it first"
                    .into(),
            );
        }
        let metadata: DraftAttachment =
            serde_json::from_str(&row.get::<String, _>("metadata_json"))?;
        if checked_relative(&metadata.filename)?.components().count() != 1 {
            return Err("Invalid managed resource filename".into());
        }
        let relative = format!(
            "mxr.draft-attachments/{}/final/{}",
            metadata.id, metadata.filename
        );
        let original = source.join(&relative);
        let target = stage.join(&relative);
        std::fs::create_dir_all(target.parent().ok_or("Resource has no parent")?)?;
        // file_metadata checks every parent, not just the final file.
        let actual = file_metadata(source, &relative)?;
        if actual.length != metadata.length_bytes || actual.sha256 != metadata.sha256 {
            return Err("Incomplete backup: managed resource metadata does not match bytes".into());
        }
        std::fs::copy(original, target)?;
        files.push(file_metadata(stage, &relative)?);
    }
    Ok(files)
}

pub async fn remap_restored(db: &Path, destination: &Path) -> BackupResult<()> {
    let mut db =
        SqliteConnection::connect_with(&sqlx::sqlite::SqliteConnectOptions::new().filename(db))
            .await?;
    for row in sqlx::query("SELECT id,local_path FROM attachments WHERE local_path IS NOT NULL")
        .fetch_all(&mut db)
        .await?
    {
        let path: String = row.get("local_path");
        let target = destination.join(checked_relative(&path)?);
        sqlx::query("UPDATE attachments SET local_path=? WHERE id=?")
            .bind(target.to_string_lossy().as_ref())
            .bind(row.get::<String, _>("id"))
            .execute(&mut db)
            .await?;
    }
    for (table, key, column, objects) in [
        ("drafts", "id", "attachments", false),
        ("drafts", "id", "inline_assets", true),
    ] {
        for row in sqlx::query(AssertSqlSafe(format!("SELECT {key},{column} FROM {table}")))
            .fetch_all(&mut db)
            .await?
        {
            let mut value: Value = serde_json::from_str(&row.get::<String, _>(column))?;
            if let Some(items) = value.as_array_mut() {
                for item in items {
                    let field = if column == "attachments" {
                        "local_path"
                    } else {
                        "path"
                    };
                    let path = if objects {
                        item.get(field).and_then(Value::as_str)
                    } else {
                        item.as_str()
                    };
                    if let Some(path) = path {
                        let absolute = destination
                            .join(checked_relative(path)?)
                            .to_string_lossy()
                            .into_owned();
                        if objects {
                            item[field] = Value::String(absolute);
                        } else {
                            *item = Value::String(absolute);
                        }
                    }
                }
            }
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE {table} SET {column}=? WHERE {key}=?"
            )))
            .bind(serde_json::to_string(&value)?)
            .bind(row.get::<String, _>(key))
            .execute(&mut db)
            .await?;
        }
    }
    db.close().await?;
    Ok(())
}

pub(super) async fn verify_references(
    db: &mut SqliteConnection,
    files: &[BackupFile],
) -> BackupResult<()> {
    let manifest: HashMap<&str, &BackupFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    let require = |path: &str| -> BackupResult<()> {
        checked_relative(path)?;
        if !manifest.contains_key(path) {
            return Err(
                format!("Incomplete backup: referenced file absent from manifest: {path}").into(),
            );
        }
        Ok(())
    };
    for row in sqlx::query("SELECT local_path FROM attachments WHERE local_path IS NOT NULL")
        .fetch_all(&mut *db)
        .await?
    {
        require(&row.get::<String, _>("local_path"))?;
    }
    for (table, column, objects) in [
        ("drafts", "attachments", false),
        ("drafts", "inline_assets", true),
    ] {
        for row in sqlx::query(AssertSqlSafe(format!("SELECT {column} FROM {table}")))
            .fetch_all(&mut *db)
            .await?
        {
            let value: Value = serde_json::from_str(&row.get::<String, _>(column))?;
            let items = value
                .as_array()
                .ok_or("Invalid path-bearing attachment list")?;
            for item in items {
                let path = if objects {
                    item.get(if column == "attachments" {
                        "local_path"
                    } else {
                        "path"
                    })
                    .and_then(Value::as_str)
                } else {
                    item.as_str()
                };
                if let Some(path) = path {
                    require(path)?;
                }
            }
        }
    }
    let mut resources = HashMap::new();
    for row in sqlx::query(
        "SELECT id,draft_id,account_id,metadata_json,ready FROM draft_attachment_resources",
    )
    .fetch_all(&mut *db)
    .await?
    {
        if !row.get::<bool, _>("ready") {
            return Err("Incomplete backup: resource upload unfinished".into());
        }
        let metadata: DraftAttachment =
            serde_json::from_str(&row.get::<String, _>("metadata_json"))?;
        if row.get::<String, _>("id") != metadata.id.as_str()
            || row.get::<String, _>("draft_id") != metadata.draft_id.as_str()
            || row.get::<String, _>("account_id") != metadata.account_id.as_str()
        {
            return Err("Managed resource identity mismatch".into());
        }
        if checked_relative(&metadata.filename)?.components().count() != 1 {
            return Err("Invalid managed resource filename".into());
        }
        let path = format!(
            "mxr.draft-attachments/{}/final/{}",
            metadata.id, metadata.filename
        );
        let file = manifest
            .get(path.as_str())
            .ok_or("Incomplete backup: managed resource absent from manifest")?;
        if file.length != metadata.length_bytes || file.sha256 != metadata.sha256 {
            return Err("Managed resource length/hash differs from declaration".into());
        }
        resources.insert(metadata.id.clone(), metadata);
    }
    for row in sqlx::query("SELECT id,account_id,managed_attachments FROM drafts")
        .fetch_all(&mut *db)
        .await?
    {
        let declarations: Option<Vec<DraftAttachment>> =
            serde_json::from_str(&row.get::<String, _>("managed_attachments"))?;
        for metadata in declarations.into_iter().flatten() {
            if metadata.draft_id.as_str() != row.get::<String, _>("id")
                || metadata.account_id.as_str() != row.get::<String, _>("account_id")
                || resources.get(&metadata.id) != Some(&metadata)
            {
                return Err("Incomplete backup: draft resource declaration or owner differs from stored resource".into());
            }
        }
    }
    Ok(())
}
