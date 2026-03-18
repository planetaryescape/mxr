use crate::state::AppState;
use mxr_protocol::*;
use std::sync::Arc;

pub async fn handle_request(state: &Arc<AppState>, msg: &IpcMessage) -> IpcMessage {
    let response_data = match &msg.payload {
        IpcPayload::Request(req) => dispatch(state, req).await,
        _ => Response::Error {
            message: "Expected a Request".to_string(),
        },
    };

    IpcMessage {
        id: msg.id,
        payload: IpcPayload::Response(response_data),
    }
}

async fn dispatch(state: &Arc<AppState>, req: &Request) -> Response {
    match req {
        Request::ListEnvelopes {
            label_id: _,
            account_id: _,
            limit,
            offset,
        } => {
            match state
                .store
                .list_envelopes_by_account(state.provider.account_id(), *limit, *offset)
                .await
            {
                Ok(envelopes) => Response::Ok {
                    data: ResponseData::Envelopes { envelopes },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetEnvelope { message_id } => match state.store.get_envelope(message_id).await {
            Ok(Some(envelope)) => Response::Ok {
                data: ResponseData::Envelope { envelope },
            },
            Ok(None) => Response::Error {
                message: "Not found".to_string(),
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::GetBody { message_id } => {
            match state
                .sync_engine
                .fetch_body(state.provider.as_ref(), message_id)
                .await
            {
                Ok(body) => Response::Ok {
                    data: ResponseData::Body { body },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetThread { thread_id } => match state.store.get_thread(thread_id).await {
            Ok(Some(thread)) => {
                let messages = state
                    .store
                    .get_thread_envelopes(thread_id)
                    .await
                    .unwrap_or_default();
                Response::Ok {
                    data: ResponseData::Thread { thread, messages },
                }
            }
            Ok(None) => Response::Error {
                message: "Thread not found".to_string(),
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::ListLabels { account_id } => {
            let aid = account_id.as_ref().unwrap_or(state.provider.account_id());
            match state.store.list_labels_by_account(aid).await {
                Ok(labels) => Response::Ok {
                    data: ResponseData::Labels { labels },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Search { query, limit } => {
            let search = state.search.lock().await;
            match search.search(query, *limit as usize) {
                Ok(results) => {
                    let items: Vec<SearchResultItem> = results
                        .into_iter()
                        .filter_map(|r| {
                            Some(SearchResultItem {
                                message_id: mxr_core::MessageId::from_uuid(
                                    uuid::Uuid::parse_str(&r.message_id).ok()?,
                                ),
                                account_id: mxr_core::AccountId::from_uuid(
                                    uuid::Uuid::parse_str(&r.account_id).ok()?,
                                ),
                                thread_id: mxr_core::ThreadId::from_uuid(
                                    uuid::Uuid::parse_str(&r.thread_id).ok()?,
                                ),
                                score: r.score,
                            })
                        })
                        .collect();
                    Response::Ok {
                        data: ResponseData::SearchResults { results: items },
                    }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::SyncNow { .. } => {
            match state
                .sync_engine
                .sync_account(state.provider.as_ref())
                .await
            {
                Ok(_) => Response::Ok {
                    data: ResponseData::Ack,
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Ping => Response::Ok {
            data: ResponseData::Pong,
        },

        Request::Shutdown => {
            std::process::exit(0);
        }

        _ => Response::Error {
            message: "Not implemented yet".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatch_ping_returns_pong() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Ping),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Pong,
            }) => {}
            other => panic!("Expected Pong, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Initial sync
        state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert_eq!(envelopes.len(), 55);
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }
}
