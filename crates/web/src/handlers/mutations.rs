use super::super::*;

macro_rules! simple_mutation_handler {
    ($name:ident, $variant:ident) => {
        pub(crate) async fn $name(
            State(state): State<AppState>,
            headers: HeaderMap,
            Query(auth): Query<AuthQuery>,
            Json(request): Json<MessageIdsRequest>,
        ) -> Result<Json<serde_json::Value>, BridgeError> {
            ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
            ack_mutation(
                &state.config.socket_path,
                mxr_protocol::MutationCommand::$variant {
                    message_ids: parse_message_ids(&request.message_ids)?,
                },
            )
            .await
        }
    };
}

simple_mutation_handler!(archive, Archive);
simple_mutation_handler!(trash, Trash);
simple_mutation_handler!(spam, Spam);
simple_mutation_handler!(mark_read_and_archive, ReadAndArchive);

pub(crate) async fn star(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<StarRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Star {
            message_ids: parse_message_ids(&request.message_ids)?,
            starred: request.starred,
        },
    )
    .await
}

pub(crate) async fn mark_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ReadRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::SetRead {
            message_ids: parse_message_ids(&request.message_ids)?,
            read: request.read,
        },
    )
    .await
}

pub(crate) async fn modify_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ModifyLabelsRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::ModifyLabels {
            message_ids: parse_message_ids(&request.message_ids)?,
            add: request.add,
            remove: request.remove,
        },
    )
    .await
}

pub(crate) async fn move_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<MoveRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Move {
            message_ids: parse_message_ids(&request.message_ids)?,
            target_label: request.target_label,
        },
    )
    .await
}
