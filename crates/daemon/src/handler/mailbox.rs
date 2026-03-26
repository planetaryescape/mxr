use super::{
    find_label_by_name, materialize_attachment_file, open_local_file,
    populate_envelope_label_provider_ids, HandlerResult,
};
use crate::mxr_core::id::{AccountId, AttachmentId, LabelId, MessageId, ThreadId};
use crate::mxr_core::types::{
    HtmlImageAsset, HtmlImageAssetStatus, HtmlImageSourceKind, MessageBody,
};
use crate::mxr_protocol::ResponseData;
use crate::state::AppState;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn resolve_account_id(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<AccountId, String> {
    account_id
        .cloned()
        .or_else(|| state.default_account_id_opt())
        .ok_or_else(|| "No sync-capable accounts configured".to_string())
}

pub(super) async fn list_envelopes(
    state: &Arc<AppState>,
    label_id: Option<&LabelId>,
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> HandlerResult {
    let result = if let Some(label_id) = label_id {
        tracing::debug!(label_id = %label_id, limit, offset, "listing envelopes by label");
        state
            .store
            .list_envelopes_by_label(label_id, limit, offset)
            .await
    } else {
        let Some(default_account_id) = state.default_account_id_opt() else {
            return Ok(ResponseData::Envelopes {
                envelopes: Vec::new(),
            });
        };
        state
            .store
            .list_envelopes_by_account(account_id.unwrap_or(&default_account_id), limit, offset)
            .await
    };

    let mut envelopes = result.map_err(|e| e.to_string())?;
    for envelope in &mut envelopes {
        if let Ok(labels) = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
        {
            let _ = populate_envelope_label_provider_ids(state, envelope, &labels).await;
        }
    }

    tracing::debug!(
        count = envelopes.len(),
        by_label = label_id.is_some(),
        "listed envelopes"
    );
    Ok(ResponseData::Envelopes { envelopes })
}

pub(super) async fn list_envelopes_by_ids(
    state: &Arc<AppState>,
    message_ids: &[MessageId],
) -> HandlerResult {
    let mut envelopes = state
        .store
        .list_envelopes_by_ids(message_ids)
        .await
        .map_err(|e| e.to_string())?;
    for envelope in &mut envelopes {
        if let Ok(labels) = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
        {
            let _ = populate_envelope_label_provider_ids(state, envelope, &labels).await;
        }
    }
    Ok(ResponseData::Envelopes { envelopes })
}

pub(super) async fn get_envelope(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
    match state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(mut envelope) => {
            if let Ok(labels) = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
            {
                let _ = populate_envelope_label_provider_ids(state, &mut envelope, &labels).await;
            }
            Ok(ResponseData::Envelope { envelope })
        }
        None => Err(format!("Envelope not found: {message_id}")),
    }
}

pub(super) async fn get_body(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
    let body = state
        .sync_engine
        .get_body(message_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Body { body })
}

pub(super) async fn get_html_image_assets(
    state: &Arc<AppState>,
    message_id: &MessageId,
    allow_remote: bool,
) -> HandlerResult {
    let body = state
        .sync_engine
        .get_body(message_id)
        .await
        .map_err(|e| e.to_string())?;
    let Some(html) = body.text_html.as_deref() else {
        return Ok(ResponseData::HtmlImageAssets {
            message_id: message_id.clone(),
            assets: Vec::new(),
        });
    };

    let mut assets = Vec::new();
    for source in collect_html_image_sources(html) {
        assets.push(resolve_html_image_asset(state, message_id, &body, &source, allow_remote).await);
    }

    Ok(ResponseData::HtmlImageAssets {
        message_id: message_id.clone(),
        assets,
    })
}

pub(super) async fn download_attachment(
    state: &Arc<AppState>,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> HandlerResult {
    let file = materialize_attachment_file(state, message_id, attachment_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::AttachmentFile { file })
}

pub(super) async fn open_attachment(
    state: &Arc<AppState>,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> HandlerResult {
    let file = materialize_attachment_file(state, message_id, attachment_id)
        .await
        .map_err(|e| e.to_string())?;
    open_local_file(&file.path).map_err(|e| e.to_string())?;
    Ok(ResponseData::AttachmentFile { file })
}

fn collect_html_image_sources(html: &str) -> Vec<String> {
    static IMG_SELECTOR: std::sync::OnceLock<Selector> = std::sync::OnceLock::new();
    let selector = IMG_SELECTOR.get_or_init(|| Selector::parse("img").expect("valid img selector"));

    let document = Html::parse_fragment(html);
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    for image in document.select(selector) {
        let Some(src) = image.value().attr("src").map(str::trim) else {
            continue;
        };
        if src.is_empty() || !seen.insert(src.to_string()) {
            continue;
        }
        sources.push(src.to_string());
    }
    sources
}

async fn resolve_html_image_asset(
    state: &Arc<AppState>,
    message_id: &MessageId,
    body: &MessageBody,
    source: &str,
    allow_remote: bool,
) -> HtmlImageAsset {
    let (kind, normalized_source) = classify_html_image_source(source);

    match kind {
        HtmlImageSourceKind::DataUri => {
            match materialize_data_uri_asset(state, message_id, normalized_source.as_str()).await {
                Ok((path, mime_type)) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Ready,
                    mime_type,
                    path: Some(path),
                    detail: None,
                },
                Err(detail) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: None,
                    path: None,
                    detail: Some(detail),
                },
            }
        }
        HtmlImageSourceKind::Remote => {
            if !allow_remote {
                return HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Blocked,
                    mime_type: None,
                    path: None,
                    detail: Some("remote content disabled".into()),
                };
            }

            match materialize_remote_asset(state, message_id, normalized_source.as_str()).await {
                Ok((path, mime_type)) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Ready,
                    mime_type,
                    path: Some(path),
                    detail: None,
                },
                Err(detail) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: None,
                    path: None,
                    detail: Some(detail),
                },
            }
        }
        HtmlImageSourceKind::Cid | HtmlImageSourceKind::ContentLocation | HtmlImageSourceKind::File => {
            let attachment = match find_html_image_attachment(body, normalized_source.as_str(), kind) {
                Some(attachment) => attachment,
                None => {
                    return HtmlImageAsset {
                        source: source.to_string(),
                        kind,
                        status: HtmlImageAssetStatus::Missing,
                        mime_type: None,
                        path: None,
                        detail: Some("no matching inline attachment".into()),
                    };
                }
            };

            match materialize_attachment_file(state, message_id, &attachment.id).await {
                Ok(file) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Ready,
                    mime_type: Some(attachment.mime_type.clone()),
                    path: Some(PathBuf::from(file.path)),
                    detail: None,
                },
                Err(error) => HtmlImageAsset {
                    source: source.to_string(),
                    kind,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: Some(attachment.mime_type.clone()),
                    path: None,
                    detail: Some(error.to_string()),
                },
            }
        }
    }
}

fn classify_html_image_source(source: &str) -> (HtmlImageSourceKind, String) {
    let trimmed = source.trim();
    if trimmed.starts_with("data:") {
        return (HtmlImageSourceKind::DataUri, trimmed.to_string());
    }
    if let Some(remote) = normalize_remote_source(trimmed) {
        return (HtmlImageSourceKind::Remote, remote);
    }
    if let Some(cid) = trimmed.strip_prefix("cid:") {
        return (HtmlImageSourceKind::Cid, normalize_content_id(cid));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return (HtmlImageSourceKind::ContentLocation, trimmed.to_string());
    }
    (HtmlImageSourceKind::File, trimmed.to_string())
}

fn normalize_remote_source(source: &str) -> Option<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        return Some(source.to_string());
    }
    source.strip_prefix("//").map(|rest| format!("https://{rest}"))
}

fn normalize_content_id(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

fn find_html_image_attachment<'a>(
    body: &'a MessageBody,
    source: &str,
    kind: HtmlImageSourceKind,
) -> Option<&'a crate::mxr_core::types::AttachmentMeta> {
    match kind {
        HtmlImageSourceKind::Cid => body.attachments.iter().find(|attachment| {
            attachment
                .content_id
                .as_deref()
                .map(normalize_content_id)
                .is_some_and(|content_id| content_id.eq_ignore_ascii_case(source))
        }),
        HtmlImageSourceKind::ContentLocation => body.attachments.iter().find(|attachment| {
            attachment
                .content_location
                .as_deref()
                .is_some_and(|location| locations_match(location, source))
                || filename_matches(&attachment.filename, source)
        }),
        HtmlImageSourceKind::File => body
            .attachments
            .iter()
            .find(|attachment| filename_matches(&attachment.filename, source)),
        HtmlImageSourceKind::DataUri | HtmlImageSourceKind::Remote => None,
    }
}

fn locations_match(left: &str, right: &str) -> bool {
    left == right
        || normalize_location_tail(left)
            .zip(normalize_location_tail(right))
            .is_some_and(|(left_tail, right_tail)| left_tail.eq_ignore_ascii_case(&right_tail))
}

fn normalize_location_tail(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        return url
            .path_segments()
            .and_then(|segments| segments.last())
            .map(|segment| segment.to_string());
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|segment| segment.to_str())
        .map(|segment| segment.to_string())
}

fn filename_matches(filename: &str, source: &str) -> bool {
    Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(source))
}

async fn materialize_data_uri_asset(
    state: &Arc<AppState>,
    message_id: &MessageId,
    source: &str,
) -> Result<(PathBuf, Option<String>), String> {
    let data_url = data_url::DataUrl::process(source).map_err(|error| error.to_string())?;
    let mime_type = Some(data_url.mime_type().to_string());
    let bytes = match data_url.decode_to_vec() {
        Ok((bytes, _)) => bytes,
        Err(error) => decode_base64_data_uri_fallback(source)
            .map_err(|fallback_error| format!("{error}; fallback decode failed: {fallback_error}"))?,
    };

    let target_dir = html_image_dir(state, message_id).join("data");
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|error| error.to_string())?;
    let path = target_dir.join(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, source.as_bytes()).to_string());
    if !path.exists() {
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok((path, mime_type))
}

fn decode_base64_data_uri_fallback(source: &str) -> Result<Vec<u8>, String> {
    let (header, body) = source
        .split_once(',')
        .ok_or_else(|| "data url is missing body delimiter".to_string())?;
    if !header.to_ascii_lowercase().contains(";base64") {
        return Err("fallback only supports base64 data urls".into());
    }

    let encoded = body
        .split('#')
        .next()
        .unwrap_or(body)
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();

    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&encoded))
        .map_err(|error| error.to_string())
}

async fn materialize_remote_asset(
    state: &Arc<AppState>,
    message_id: &MessageId,
    source: &str,
) -> Result<(PathBuf, Option<String>), String> {
    let target_dir = html_image_dir(state, message_id).join("remote");
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|error| error.to_string())?;
    let path = target_dir.join(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, source.as_bytes()).to_string());
    if path.exists() {
        return Ok((path, None));
    }

    let response = reqwest::Client::new()
        .get(source)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("remote image fetch failed: {}", response.status()));
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string());
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| error.to_string())?;
    Ok((path, mime_type))
}

fn html_image_dir(state: &AppState, message_id: &MessageId) -> PathBuf {
    state
        .attachment_dir()
        .join("_html_assets")
        .join(message_id.as_str())
}

pub(super) async fn list_bodies(state: &Arc<AppState>, message_ids: &[MessageId]) -> HandlerResult {
    tracing::debug!(count = message_ids.len(), "ListBodies: fetching bodies");
    let mut bodies = Vec::with_capacity(message_ids.len());
    for id in message_ids {
        if let Ok(Some(full)) = state.store.get_body(id).await {
            let (plain, html) = if full.text_plain.is_some() {
                (full.text_plain, None)
            } else {
                (None, full.text_html)
            };
            bodies.push(crate::mxr_core::types::MessageBody {
                message_id: full.message_id,
                text_plain: plain,
                text_html: html,
                attachments: full.attachments,
                fetched_at: full.fetched_at,
                metadata: full.metadata,
            });
        }
    }
    Ok(ResponseData::Bodies { bodies })
}

pub(super) async fn get_thread(state: &Arc<AppState>, thread_id: &ThreadId) -> HandlerResult {
    let thread = state
        .store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;
    let mut messages = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .unwrap_or_default();
    if let Ok(labels) = state.store.list_labels_by_account(&thread.account_id).await {
        for message in &mut messages {
            let _ = populate_envelope_label_provider_ids(state, message, &labels).await;
        }
    }
    Ok(ResponseData::Thread { thread, messages })
}

pub(super) async fn list_labels(
    state: &Arc<AppState>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let Some(account_id) = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt())
    else {
        return Ok(ResponseData::Labels { labels: Vec::new() });
    };
    let labels = state
        .store
        .list_labels_by_account(&account_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Labels { labels })
}

pub(super) async fn create_label(
    state: &Arc<AppState>,
    name: &str,
    color: Option<&str>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let provider = state.get_provider(Some(&account_id))?;
    let mut label = provider
        .create_label(name, color)
        .await
        .map_err(|e| e.to_string())?;
    if label.account_id != account_id {
        label.account_id = account_id;
    }
    state
        .store
        .upsert_label(&label)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Label { label })
}

pub(super) async fn delete_label(
    state: &Arc<AppState>,
    name: &str,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let label = find_label_by_name(state, &account_id, name).await?;
    let provider = state.get_provider(Some(&account_id))?;
    provider
        .delete_label(&label.provider_id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .delete_label(&label.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn rename_label(
    state: &Arc<AppState>,
    old: &str,
    new: &str,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let existing = find_label_by_name(state, &account_id, old).await?;
    let provider = state.get_provider(Some(&account_id))?;
    let mut label = provider
        .rename_label(&existing.provider_id, new)
        .await
        .map_err(|e| e.to_string())?;
    if label.account_id != account_id {
        label.account_id = account_id.clone();
    }
    state
        .store
        .replace_label(&existing.id, &label)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Label { label })
}
