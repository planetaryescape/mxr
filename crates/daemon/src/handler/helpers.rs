use crate::state::AppState;
use mxr_rules::{DryRunResult, Rule, RuleEngine};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub(super) fn protocol_event_entry(entry: mxr_store::EventLogEntry) -> mxr_protocol::EventLogEntry {
    mxr_protocol::EventLogEntry {
        timestamp: entry.timestamp,
        level: entry.level,
        category: entry.category,
        account_id: entry.account_id,
        message_id: entry.message_id,
        rule_id: entry.rule_id,
        summary: entry.summary,
        details: entry.details,
    }
}

pub(crate) fn recent_log_lines_sync(
    limit: usize,
    level: Option<&str>,
) -> Result<Vec<String>, std::io::Error> {
    let log_path = mxr_config::data_dir().join("logs").join("mxr.log");
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(log_path)?;
    let mut lines = BufReader::new(file)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(level) = level {
        let level = level.to_ascii_lowercase();
        lines.retain(|line| line.to_ascii_lowercase().contains(&level));
    }
    let start = lines.len().saturating_sub(limit.max(1));
    Ok(lines.split_off(start))
}

pub(super) async fn recent_log_lines(
    state: &AppState,
    limit: usize,
    level: Option<&str>,
) -> Result<Vec<String>, String> {
    let level = level.map(str::to_string);
    let mut lines = run_admin_blocking(state, "recent_log_lines", move || {
        recent_log_lines_sync(limit, level.as_deref()).map_err(|error| error.to_string())
    })
    .await?;
    if lines.is_empty() {
        lines.push("(no recent logs)".to_string());
    }
    Ok(lines)
}

pub(super) async fn file_size(state: &AppState, path: PathBuf) -> u64 {
    run_admin_blocking(state, "file_size", move || Ok(file_size_sync(&path)))
        .await
        .unwrap_or(0)
}

pub(super) async fn dir_size(state: &AppState, path: PathBuf) -> u64 {
    run_admin_blocking(state, "dir_size", move || Ok(dir_size_sync(&path)))
        .await
        .unwrap_or(0)
}

async fn run_admin_blocking<T, F>(
    state: &AppState,
    operation: &'static str,
    task: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let _permit = state.acquire_admin_blocking_permit().await?;
    tokio::task::spawn_blocking(move || {
        let started_at = std::time::Instant::now();
        let result = task();
        tracing::trace!(
            operation,
            elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
            "daemon admin blocking task completed"
        );
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

pub(crate) fn dir_size_sync(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(metadata) if metadata.is_dir() => std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .map(|path| {
                if path.is_dir() {
                    dir_size_sync(&path)
                } else {
                    file_size_sync(&path)
                }
            })
            .sum(),
        _ => 0,
    }
}

pub(crate) fn file_size_sync(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub(crate) fn should_fallback_to_tantivy(query: &str, error: &mxr_search::ParseError) -> bool {
    if looks_structured_query(query) {
        return false;
    }

    matches!(
        error,
        mxr_search::ParseError::UnexpectedToken(_)
            | mxr_search::ParseError::UnexpectedEnd
            | mxr_search::ParseError::UnmatchedParen
    )
}

fn looks_structured_query(query: &str) -> bool {
    let trimmed = query.trim();
    trimmed.contains(':')
        || trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.starts_with('-')
        || trimmed.contains(" AND ")
        || trimmed.contains(" OR ")
        || trimmed.contains(" NOT ")
}

pub(super) async fn persist_rule(state: &AppState, rule: &Rule) -> Result<(), String> {
    let conditions_json = serde_json::to_string(&rule.conditions).map_err(|e| e.to_string())?;
    let actions_json = serde_json::to_string(&rule.actions).map_err(|e| e.to_string())?;
    state
        .store
        .upsert_rule(mxr_store::RuleRecordInput {
            id: &rule.id.0,
            name: &rule.name,
            enabled: rule.enabled,
            priority: rule.priority,
            conditions_json: &conditions_json,
            actions_json: &actions_json,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        })
        .await
        .map_err(|e| e.to_string())
}

fn row_to_rule(row: &sqlx::sqlite::SqliteRow) -> Result<Rule, String> {
    serde_json::from_value(mxr_store::row_to_rule_json(row)).map_err(|e| e.to_string())
}

pub(super) async fn dry_run_rules(
    state: &AppState,
    rule_key: Option<String>,
    all: bool,
    after: Option<String>,
) -> Result<Vec<DryRunResult>, String> {
    let rows = if all {
        state.store.list_rules().await.map_err(|e| e.to_string())?
    } else if let Some(rule_key) = rule_key {
        match state
            .store
            .get_rule_by_id_or_name(&rule_key)
            .await
            .map_err(|e| e.to_string())?
        {
            Some(row) => vec![row],
            None => return Err(format!("Rule not found: {rule_key}")),
        }
    } else {
        return Err("Provide a rule or use --all".to_string());
    };

    let rules: Vec<Rule> = rows.iter().map(row_to_rule).collect::<Result<_, _>>()?;
    let engine = RuleEngine::new(rules.clone());
    let after = after
        .map(|value| {
            chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map(|date| date.and_time(chrono::NaiveTime::MIN))
                .map(|dt| {
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)
                })
                .map_err(|e| e.to_string())
        })
        .transpose()?;

    let mut owned_messages = Vec::new();
    for account in state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?
    {
        let labels = state
            .store
            .list_labels_by_account(&account.id)
            .await
            .map_err(|e| e.to_string())?;
        let envelopes = state
            .store
            .list_envelopes_by_account(&account.id, 10_000, 0)
            .await
            .map_err(|e| e.to_string())?;
        for envelope in envelopes {
            if after.is_some_and(|cutoff| envelope.date < cutoff) {
                continue;
            }
            let body = state
                .store
                .get_body(&envelope.id)
                .await
                .map_err(|e| e.to_string())?;
            let label_ids = state
                .store
                .get_message_label_ids(&envelope.id)
                .await
                .map_err(|e| e.to_string())?;
            let visible_labels = labels
                .iter()
                .filter(|label| label_ids.iter().any(|id| id == &label.id))
                .map(|label| label.provider_id.clone())
                .collect();
            owned_messages.push(DryRunMessage::from_parts(envelope, body, visible_labels));
        }
    }

    let dry_run_input: Vec<_> = owned_messages
        .iter()
        .map(|message| {
            (
                message as &dyn mxr_rules::MessageView,
                message.id.as_str(),
                message.from.as_str(),
                message.subject.as_str(),
            )
        })
        .collect();

    if all {
        Ok(rules
            .iter()
            .filter(|rule| rule.enabled)
            .filter_map(|rule| engine.dry_run(&rule.id, &dry_run_input))
            .collect())
    } else {
        Ok(rules
            .first()
            .and_then(|rule| engine.dry_run(&rule.id, &dry_run_input))
            .into_iter()
            .collect())
    }
}

struct DryRunMessage {
    id: String,
    from: String,
    to: Vec<String>,
    subject: String,
    labels: Vec<String>,
    has_attachment: bool,
    size_bytes: u64,
    date: chrono::DateTime<chrono::Utc>,
    is_unread: bool,
    is_starred: bool,
    has_unsubscribe: bool,
    body_text: Option<String>,
}

impl DryRunMessage {
    fn from_parts(
        envelope: mxr_core::Envelope,
        body: Option<mxr_core::MessageBody>,
        labels: Vec<String>,
    ) -> Self {
        Self {
            id: envelope.id.to_string(),
            from: envelope.from.email,
            to: envelope.to.into_iter().map(|addr| addr.email).collect(),
            subject: envelope.subject,
            labels,
            has_attachment: envelope.has_attachments,
            size_bytes: envelope.size_bytes,
            date: envelope.date,
            is_unread: !envelope.flags.contains(mxr_core::MessageFlags::READ),
            is_starred: envelope.flags.contains(mxr_core::MessageFlags::STARRED),
            has_unsubscribe: !matches!(
                envelope.unsubscribe,
                mxr_core::types::UnsubscribeMethod::None
            ),
            body_text: body.and_then(|body| body.text_plain.or(body.text_html)),
        }
    }
}

impl mxr_rules::MessageView for DryRunMessage {
    fn sender_email(&self) -> &str {
        &self.from
    }

    fn to_emails(&self) -> &[String] {
        &self.to
    }

    fn subject(&self) -> &str {
        &self.subject
    }

    fn labels(&self) -> &[String] {
        &self.labels
    }

    fn has_attachment(&self) -> bool {
        self.has_attachment
    }

    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    fn date(&self) -> chrono::DateTime<chrono::Utc> {
        self.date
    }

    fn is_unread(&self) -> bool {
        self.is_unread
    }

    fn is_starred(&self) -> bool {
        self.is_starred
    }

    fn has_unsubscribe(&self) -> bool {
        self.has_unsubscribe
    }

    fn body_text(&self) -> Option<&str> {
        self.body_text.as_deref()
    }
}
