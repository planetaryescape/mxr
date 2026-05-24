//! Handlers for the `*Activity` IPC verbs. Reads delegate to the store
//! repo; mutating verbs additionally emit a synthesized marker so the
//! activity log records its own administration.

use mxr_protocol::{
    ActivityCursor, ActivityEntry, ActivityExportFormat, ActivityFilter, ActivityStatBucket,
    ActivityStatGroupBy, ActivityTier, ClientKind, ResponseData, SavedActivityFilterEntry,
};
use mxr_store as store;
use std::sync::Arc;

use crate::activity::{current_unix_ms, OwnedEntry};
use crate::handler::HandlerError;
use crate::state::AppState;

type HandlerResult = Result<ResponseData, HandlerError>;

const INLINE_EXPORT_CAP_BYTES: usize = 1_048_576; // 1 MiB

// ===== conversions =====

fn proto_tier_to_store(t: ActivityTier) -> store::Tier {
    match t {
        ActivityTier::Ephemeral => store::Tier::Ephemeral,
        ActivityTier::Standard => store::Tier::Standard,
        ActivityTier::Important => store::Tier::Important,
    }
}

fn store_tier_to_proto(s: &str) -> ActivityTier {
    match s {
        "ephemeral" => ActivityTier::Ephemeral,
        "important" => ActivityTier::Important,
        _ => ActivityTier::Standard,
    }
}

fn proto_source_to_str(c: &ClientKind) -> String {
    c.as_str().to_owned()
}

fn proto_filter_to_store(f: &ActivityFilter) -> store::ActivityFilter {
    store::ActivityFilter {
        since: f.since,
        until: f.until,
        account_id: f.account_id.clone(),
        sources: f.sources.iter().map(proto_source_to_str).collect(),
        actions: f.actions.clone(),
        action_prefix: f.action_prefix.clone(),
        target_kind: f.target_kind.clone(),
        target_id: f.target_id.clone(),
        tiers: f.tiers.iter().map(|t| t.as_str().to_owned()).collect(),
        query: f.query.clone(),
        include_redacted: f.include_redacted,
    }
}

fn store_row_to_proto(r: &store::ActivityRow) -> ActivityEntry {
    let source = match r.source.as_str() {
        "tui" => ClientKind::Tui,
        "web" => ClientKind::Web,
        "daemon" => ClientKind::Daemon,
        _ => ClientKind::Cli,
    };
    let context = r
        .context_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    ActivityEntry {
        id: r.id,
        ts: r.ts,
        account_id: r.account_id.clone(),
        source,
        action: r.action.clone(),
        target_kind: r.target_kind.clone(),
        target_id: r.target_id.clone(),
        tier: store_tier_to_proto(&r.tier),
        context,
        redacted: r.redacted,
    }
}

// ===== handlers =====

pub(super) async fn list_activity(
    state: &Arc<AppState>,
    filter: &ActivityFilter,
    limit: u32,
    cursor: Option<ActivityCursor>,
) -> HandlerResult {
    let store_filter = proto_filter_to_store(filter);
    let store_cursor = cursor.map(|c| store::ActivityCursor { ts: c.ts, id: c.id });
    let page = state
        .store
        .list_activity(&store_filter, limit, store_cursor)
        .await
        ?;
    let entries: Vec<ActivityEntry> = page.rows.iter().map(store_row_to_proto).collect();
    let next_cursor = page
        .next_cursor
        .map(|c| ActivityCursor { ts: c.ts, id: c.id });
    Ok(ResponseData::ActivityEntries {
        entries,
        next_cursor,
    })
}

pub(super) async fn count_activity(
    state: &Arc<AppState>,
    filter: &ActivityFilter,
) -> HandlerResult {
    let store_filter = proto_filter_to_store(filter);
    let count = state
        .store
        .count_activity(&store_filter)
        .await
        ?;
    Ok(ResponseData::ActivityCount { count })
}

pub(super) async fn activity_stats(
    state: &Arc<AppState>,
    since: i64,
    until: i64,
    group_by: ActivityStatGroupBy,
) -> HandlerResult {
    let pairs = match group_by {
        ActivityStatGroupBy::Action => state.store.activity_stats_by_action(since, until).await,
        ActivityStatGroupBy::Day => state.store.activity_stats_by_day(since, until).await,
        ActivityStatGroupBy::Source => state.store.activity_stats_by_source(since, until).await,
        ActivityStatGroupBy::TargetKind => {
            state
                .store
                .activity_stats_by_target_kind(since, until)
                .await
        }
        ActivityStatGroupBy::Hour => state.store.activity_stats_by_hour(since, until).await,
    }
    ?;
    let buckets = pairs
        .into_iter()
        .map(|(key, count)| ActivityStatBucket { key, count })
        .collect();
    Ok(ResponseData::ActivityStatBuckets { buckets })
}

pub(super) async fn export_activity(
    state: &Arc<AppState>,
    filter: &ActivityFilter,
    format: ActivityExportFormat,
    path: Option<String>,
) -> HandlerResult {
    let store_filter = proto_filter_to_store(filter);
    // Stream rows in chunks of 1k via cursor pagination.
    let mut all_rows: Vec<store::ActivityRow> = Vec::new();
    let mut cursor: Option<store::ActivityCursor> = None;
    loop {
        let page = state
            .store
            .list_activity(&store_filter, 1000, cursor)
            .await
            ?;
        let next = page.next_cursor;
        all_rows.extend(page.rows);
        match next {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    let entries: Vec<ActivityEntry> = all_rows.iter().map(store_row_to_proto).collect();
    let count = entries.len() as i64;

    let body = match format {
        ActivityExportFormat::Json => {
            serde_json::to_string_pretty(&entries)?
        }
        ActivityExportFormat::Ndjson => {
            let mut s = String::new();
            for e in &entries {
                let line = serde_json::to_string(e)?;
                s.push_str(&line);
                s.push('\n');
            }
            s
        }
        ActivityExportFormat::Csv => render_csv(&entries),
    };
    let size_bytes = body.len() as u64;

    // Synthesized marker — always emitted, even if writing fails below.
    state.activity.record(OwnedEntry {
        ts: current_unix_ms(),
        account_id: None,
        source: ClientKind::Daemon,
        action: "activity.exported".into(),
        target_kind: None,
        target_id: None,
        tier: store::Tier::Important,
        context: Some(serde_json::json!({
            "format": format_to_str(format),
            "count": count,
            "path": path,
        })),
    });

    match path {
        Some(p) => {
            // Atomic write via tmp-rename to avoid partial files.
            let tmp = format!("{p}.tmp");
            std::fs::write(&tmp, &body).map_err(|e| format!("write {p}.tmp: {e}"))?;
            std::fs::rename(&tmp, &p).map_err(|e| format!("rename {p}: {e}"))?;
            Ok(ResponseData::ActivityExportResult {
                format,
                count,
                size_bytes,
                body: None,
                path: Some(p),
            })
        }
        None if (body.len() <= INLINE_EXPORT_CAP_BYTES) => Ok(ResponseData::ActivityExportResult {
            format,
            count,
            size_bytes,
            body: Some(body),
            path: None,
        }),
        None => Err(format!(
            "inline export too large ({size_bytes} bytes); pass `path` to write to file"
        ).into()),
    }
}

fn format_to_str(f: ActivityExportFormat) -> &'static str {
    match f {
        ActivityExportFormat::Csv => "csv",
        ActivityExportFormat::Json => "json",
        ActivityExportFormat::Ndjson => "ndjson",
    }
}

/// Minimal RFC 4180 CSV writer. We use an inline writer (not the `csv`
/// crate) to avoid pulling another dep. The columns are stable and
/// documented in `docs/activity-log.md`.
fn render_csv(entries: &[ActivityEntry]) -> String {
    let mut s = String::with_capacity(64 * entries.len() + 256);
    s.push_str("id,ts,account_id,source,action,target_kind,target_id,tier,context_json,redacted\n");
    for e in entries {
        push_csv_i64(&mut s, e.id);
        s.push(',');
        push_csv_i64(&mut s, e.ts);
        s.push(',');
        push_csv_opt(&mut s, e.account_id.as_deref());
        s.push(',');
        push_csv_str(&mut s, e.source.as_str());
        s.push(',');
        push_csv_str(&mut s, &e.action);
        s.push(',');
        push_csv_opt(&mut s, e.target_kind.as_deref());
        s.push(',');
        push_csv_opt(&mut s, e.target_id.as_deref());
        s.push(',');
        push_csv_str(&mut s, e.tier.as_str());
        s.push(',');
        if let Some(v) = &e.context {
            let serialized = serde_json::to_string(v).unwrap_or_default();
            push_csv_str(&mut s, &serialized);
        }
        s.push(',');
        s.push_str(if e.redacted { "1" } else { "0" });
        s.push('\n');
    }
    s
}

fn push_csv_i64(out: &mut String, v: i64) {
    out.push_str(&v.to_string());
}

fn push_csv_opt(out: &mut String, v: Option<&str>) {
    if let Some(s) = v {
        push_csv_str(out, s);
    }
}

fn push_csv_str(out: &mut String, v: &str) {
    let needs_quote = v.contains(',') || v.contains('"') || v.contains('\n');
    if !needs_quote {
        out.push_str(v);
        return;
    }
    out.push('"');
    for ch in v.chars() {
        if ch == '"' {
            out.push_str("\"\"");
        } else {
            out.push(ch);
        }
    }
    out.push('"');
}

pub(super) async fn redact_activity(
    state: &Arc<AppState>,
    ids: &[i64],
    filter: Option<&ActivityFilter>,
    dry_run: bool,
) -> HandlerResult {
    if ids.is_empty() && filter.is_none() {
        return Err("redact requires `ids` or `filter`".into());
    }
    if !ids.is_empty() && filter.is_some() {
        return Err("redact takes `ids` OR `filter`, not both".into());
    }

    if dry_run {
        // Best-effort affected count without mutating.
        let count = if !ids.is_empty() {
            ids.len() as i64
        } else {
            let store_filter = proto_filter_to_store(filter.expect("filter checked above"));
            state
                .store
                .count_activity(&store_filter)
                .await
                ?
        };
        return Ok(ResponseData::ActivityAffected {
            count,
            dry_run: true,
        });
    }

    let (count, by) = if !ids.is_empty() {
        let n = state
            .store
            .redact_activity_by_ids(ids)
            .await
            ?;
        (n as i64, "ids")
    } else {
        let store_filter = proto_filter_to_store(filter.expect("filter checked above"));
        let n = state
            .store
            .redact_activity_by_filter(&store_filter)
            .await
            ?;
        (n as i64, "filter")
    };

    // Synthesized marker AFTER the redaction lands so the marker itself isn't redacted.
    state.activity.record(OwnedEntry {
        ts: current_unix_ms(),
        account_id: None,
        source: ClientKind::Daemon,
        action: "activity.redacted".into(),
        target_kind: None,
        target_id: None,
        tier: store::Tier::Important,
        context: Some(serde_json::json!({ "count": count, "by": by })),
    });

    Ok(ResponseData::ActivityAffected {
        count,
        dry_run: false,
    })
}

pub(super) async fn prune_activity(
    state: &Arc<AppState>,
    before_ts: i64,
    tier: Option<ActivityTier>,
    dry_run: bool,
) -> HandlerResult {
    if dry_run {
        // Count without deleting.
        let mut store_filter = store::ActivityFilter {
            until: Some(before_ts),
            include_redacted: true,
            ..Default::default()
        };
        if let Some(t) = tier {
            store_filter.tiers = vec![proto_tier_to_store(t).as_str().to_owned()];
        }
        let count = state
            .store
            .count_activity(&store_filter)
            .await
            ?;
        return Ok(ResponseData::ActivityAffected {
            count,
            dry_run: true,
        });
    }

    let store_tier = tier.map(proto_tier_to_store);
    let deleted = state
        .store
        .prune_activity_before(before_ts, store_tier)
        .await
        ?;

    state.activity.record(OwnedEntry {
        ts: current_unix_ms(),
        account_id: None,
        source: ClientKind::Daemon,
        action: "activity.pruned".into(),
        target_kind: None,
        target_id: None,
        tier: store::Tier::Important,
        context: Some(serde_json::json!({
            "before_ts": before_ts,
            "tier": tier.map(|t| t.as_str()),
            "deleted": deleted,
        })),
    });

    Ok(ResponseData::ActivityAffected {
        count: deleted as i64,
        dry_run: false,
    })
}

pub(super) async fn pause_activity(state: &Arc<AppState>, until_ts: Option<i64>) -> HandlerResult {
    state.activity.pause(until_ts);
    state.activity.record_forced(OwnedEntry {
        ts: current_unix_ms(),
        account_id: None,
        source: ClientKind::Daemon,
        action: "activity.paused".into(),
        target_kind: None,
        target_id: None,
        tier: store::Tier::Important,
        context: Some(serde_json::json!({
            "until": until_ts,
            "reason": "user_command",
        })),
    });
    Ok(ResponseData::Acknowledged)
}

// ===== saved filter handlers (Phase 8) =====

fn store_saved_to_proto(s: &store::SavedActivityFilter) -> SavedActivityFilterEntry {
    let filter: ActivityFilter = serde_json::from_str(&s.filter_json).unwrap_or_default();
    SavedActivityFilterEntry {
        slug: s.slug.clone(),
        name: s.name.clone(),
        filter,
        created_at: s.created_at,
        updated_at: s.updated_at,
        last_used_at: s.last_used_at,
    }
}

pub(super) async fn list_saved_filters(state: &Arc<AppState>) -> HandlerResult {
    let rows = state
        .store
        .list_saved_activity_filters()
        .await
        ?;
    let entries = rows.iter().map(store_saved_to_proto).collect();
    Ok(ResponseData::SavedActivityFilters { entries })
}

pub(super) async fn get_saved_filter(state: &Arc<AppState>, slug: &str) -> HandlerResult {
    let row = state
        .store
        .get_saved_activity_filter(slug)
        .await
        ?;
    let entry = row.as_ref().map(store_saved_to_proto);
    if entry.is_some() {
        let _ = state.store.mark_saved_activity_filter_used(slug).await;
    }
    Ok(ResponseData::SavedActivityFilterDetail { entry })
}

pub(super) async fn upsert_saved_filter(
    state: &Arc<AppState>,
    slug: &str,
    name: &str,
    filter: &ActivityFilter,
) -> HandlerResult {
    if slug.is_empty() {
        return Err("saved filter slug must not be empty".into());
    }
    let filter_json = serde_json::to_string(filter)?;
    state
        .store
        .upsert_saved_activity_filter(slug, name, &filter_json)
        .await
        ?;
    Ok(ResponseData::Acknowledged)
}

pub(super) async fn delete_saved_filter(state: &Arc<AppState>, slug: &str) -> HandlerResult {
    let n = state
        .store
        .delete_saved_activity_filter(slug)
        .await
        ?;
    Ok(ResponseData::ActivityAffected {
        count: n as i64,
        dry_run: false,
    })
}

pub(super) async fn resume_activity(state: &Arc<AppState>) -> HandlerResult {
    state.activity.resume();
    state.activity.record_forced(OwnedEntry {
        ts: current_unix_ms(),
        account_id: None,
        source: ClientKind::Daemon,
        action: "activity.resumed".into(),
        target_kind: None,
        target_id: None,
        tier: store::Tier::Important,
        context: Some(serde_json::json!({
            "auto": false,
            "reason": "user_command",
        })),
    });
    Ok(ResponseData::Acknowledged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    async fn state() -> Arc<AppState> {
        Arc::new(AppState::in_memory_without_accounts().await.unwrap())
    }

    fn empty_filter() -> ActivityFilter {
        ActivityFilter::default()
    }

    #[tokio::test]
    async fn list_returns_empty_when_no_rows() {
        let s = state().await;
        let resp = list_activity(&s, &empty_filter(), 50, None).await.unwrap();
        match resp {
            ResponseData::ActivityEntries {
                entries,
                next_cursor,
            } => {
                assert!(entries.is_empty());
                assert!(next_cursor.is_none());
            }
            other => panic!("wrong shape: {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_includes_records_from_recorder_path() {
        let s = state().await;
        s.activity.record(OwnedEntry {
            ts: 1_000,
            account_id: None,
            source: ClientKind::Tui,
            action: "mail.archive".into(),
            target_kind: Some("thread".into()),
            target_id: Some("thr_1".into()),
            tier: store::Tier::Important,
            context: None,
        });
        // Let the recorder worker drain.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let resp = list_activity(&s, &empty_filter(), 50, None).await.unwrap();
        match resp {
            ResponseData::ActivityEntries { entries, .. } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].action, "mail.archive");
                assert!(matches!(entries[0].source, ClientKind::Tui));
                assert!(matches!(entries[0].tier, ActivityTier::Important));
            }
            other => panic!("wrong shape: {other:?}"),
        }
    }

    #[tokio::test]
    async fn redact_with_neither_ids_nor_filter_errors() {
        let s = state().await;
        let err = redact_activity(&s, &[], None, false).await.err().unwrap();
        assert!(err.to_string().contains("redact"), "got: {err}");
    }

    #[tokio::test]
    async fn dry_run_redact_does_not_mutate() {
        let s = state().await;
        // seed
        s.store
            .record_activity(store::ActivityInsert {
                ts: 1_000,
                account_id: None,
                source: "tui",
                action: "mail.archive",
                target_kind: Some("thread"),
                target_id: Some("thr_1"),
                tier: store::Tier::Important,
                context: None,
            })
            .await
            .unwrap();

        let resp = redact_activity(&s, &[1], None, true).await.unwrap();
        match resp {
            ResponseData::ActivityAffected { count, dry_run } => {
                assert_eq!(count, 1);
                assert!(dry_run);
            }
            other => panic!("wrong shape: {other:?}"),
        }
        // Row remains un-redacted.
        let page = s
            .store
            .list_activity(&store::ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(!page.rows[0].redacted);
    }

    #[tokio::test]
    async fn pause_emits_synthesized_marker_even_while_paused() {
        let s = state().await;
        pause_activity(&s, None).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let page = s
            .store
            .list_activity(&store::ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        let actions: Vec<&str> = page.rows.iter().map(|r| r.action.as_str()).collect();
        assert!(actions.contains(&"activity.paused"));
    }

    #[tokio::test]
    async fn export_inline_returns_csv_body() {
        let s = state().await;
        s.store
            .record_activity(store::ActivityInsert {
                ts: 1_000,
                account_id: None,
                source: "tui",
                action: "mail.archive",
                target_kind: Some("thread"),
                target_id: Some("thr_1"),
                tier: store::Tier::Important,
                context: Some(&serde_json::json!({ "thread_id": "thr_1" })),
            })
            .await
            .unwrap();

        let resp = export_activity(&s, &empty_filter(), ActivityExportFormat::Csv, None)
            .await
            .unwrap();
        match resp {
            ResponseData::ActivityExportResult {
                format,
                count,
                body,
                path,
                ..
            } => {
                assert!(matches!(format, ActivityExportFormat::Csv));
                assert_eq!(count, 1);
                assert!(path.is_none());
                let body = body.unwrap();
                assert!(body.starts_with(
                    "id,ts,account_id,source,action,target_kind,target_id,tier,context_json,redacted"
                ));
                assert!(body.contains("mail.archive"));
            }
            other => panic!("wrong shape: {other:?}"),
        }
    }
}
