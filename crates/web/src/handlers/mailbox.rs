use super::super::*;

pub(crate) async fn mailbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MailboxQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    let lens = query.lens();
    let chrome = build_bridge_chrome(&state.config.socket_path, &lens).await?;
    let mailbox = load_mailbox_selection(
        &state.config.socket_path,
        &chrome,
        &lens,
        query.limit,
        query.offset,
    )
    .await?;
    Ok(Json(json!({
        "shell": chrome.shell,
        "sidebar": chrome.sidebar,
        "mailbox": {
            "lensLabel": mailbox.lens_label,
            "counts": mailbox.counts,
            "groups": group_envelopes(mailbox.envelopes),
        }
    })))
}

pub(crate) async fn thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(thread_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let thread_id = parse_thread_id(&thread_id)?;
    match ipc_request(&state.config.socket_path, Request::GetThread { thread_id }).await? {
        ResponseData::Thread { thread, messages } => {
            let bodies = match ipc_request(
                &state.config.socket_path,
                Request::ListBodies {
                    message_ids: messages
                        .iter()
                        .map(|message| message.id.clone())
                        .collect::<Vec<MessageId>>(),
                },
            )
            .await?
            {
                ResponseData::Bodies { bodies } => bodies,
                _ => return Err(BridgeError::UnexpectedResponse),
            };

            let attachment_count = bodies
                .iter()
                .map(|body| body.attachments.len())
                .sum::<usize>();

            Ok(Json(json!({
                "thread": thread,
                "messages": messages.iter().map(message_row_view).collect::<Vec<_>>(),
                "bodies": bodies,
                "reader_mode": thread_reader_mode(&bodies),
                "right_rail": {
                    "title": "Thread context",
                    "items": [
                        format!("{} messages", thread.message_count),
                        format!("{} unread", thread.unread_count),
                        format!("{} participants", thread.participants.len()),
                        if attachment_count == 0 {
                            "No attachments".to_string()
                        } else {
                            format!("{attachment_count} attachments")
                        }
                    ],
                }
            })))
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn export_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(thread_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ExportThread {
            thread_id: parse_thread_id(&thread_id)?,
            format: mxr_core::types::ExportFormat::Markdown,
        },
    )
    .await?
    {
        ResponseData::ExportResult { content } => Ok(Json(json!({ "content": content }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}
