use crate::state::AppState;
use mxr_core::types::Snoozed;
use mxr_protocol::*;
use mxr_search::{parse_query, QueryBuilder};
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
            label_id,
            account_id: _,
            limit,
            offset,
        } => {
            let result = if let Some(lid) = label_id {
                tracing::debug!(label_id = %lid, limit, offset, "listing envelopes by label");
                state
                    .store
                    .list_envelopes_by_label(lid, *limit, *offset)
                    .await
            } else {
                state
                    .store
                    .list_envelopes_by_account(state.provider.account_id(), *limit, *offset)
                    .await
            };
            match result {
                Ok(envelopes) => {
                    tracing::debug!(
                        count = envelopes.len(),
                        by_label = label_id.is_some(),
                        "listed envelopes"
                    );
                    Response::Ok {
                        data: ResponseData::Envelopes { envelopes },
                    }
                }
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
            // Try custom parser first, fall back to Tantivy's built-in parser
            let results = match parse_query(query) {
                Ok(ast) => {
                    let builder = QueryBuilder::new(search.schema());
                    let tantivy_query = builder.build(&ast);
                    search.search_ast(tantivy_query, *limit as usize)
                }
                Err(_) => search.search(query, *limit as usize),
            };
            match results {
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

        Request::Count { query } => {
            let search = state.search.lock().await;
            let results = match parse_query(query) {
                Ok(ast) => {
                    let builder = QueryBuilder::new(search.schema());
                    let tantivy_query = builder.build(&ast);
                    search.search_ast(tantivy_query, 10_000)
                }
                Err(_) => search.search(query, 10_000),
            };
            match results {
                Ok(results) => Response::Ok {
                    data: ResponseData::Count {
                        count: results.len() as u32,
                    },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetHeaders { message_id } => match state.store.get_envelope(message_id).await {
            Ok(Some(envelope)) => {
                let mut headers = Vec::new();
                headers.push((
                    "From".to_string(),
                    format!(
                        "{} <{}>",
                        envelope.from.name.as_deref().unwrap_or(""),
                        envelope.from.email
                    ),
                ));
                headers.push(("Subject".to_string(), envelope.subject.clone()));
                headers.push(("Date".to_string(), envelope.date.to_rfc3339()));
                for addr in &envelope.to {
                    headers.push((
                        "To".to_string(),
                        format!("{} <{}>", addr.name.as_deref().unwrap_or(""), addr.email),
                    ));
                }
                for addr in &envelope.cc {
                    headers.push((
                        "Cc".to_string(),
                        format!("{} <{}>", addr.name.as_deref().unwrap_or(""), addr.email),
                    ));
                }
                if let Some(ref mid) = envelope.message_id_header {
                    headers.push(("Message-ID".to_string(), mid.clone()));
                }
                if let Some(ref irt) = envelope.in_reply_to {
                    headers.push(("In-Reply-To".to_string(), irt.clone()));
                }
                Response::Ok {
                    data: ResponseData::Headers { headers },
                }
            }
            Ok(None) => Response::Error {
                message: "Not found".to_string(),
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::ListSavedSearches => match state.store.list_saved_searches().await {
            Ok(searches) => Response::Ok {
                data: ResponseData::SavedSearches { searches },
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::CreateSavedSearch { name, query } => {
            let search = mxr_core::types::SavedSearch {
                id: mxr_core::SavedSearchId::new(),
                account_id: None,
                name: name.clone(),
                query: query.clone(),
                sort: mxr_core::types::SortOrder::DateDesc,
                icon: None,
                position: 0,
                created_at: chrono::Utc::now(),
            };
            match state.store.insert_saved_search(&search).await {
                Ok(()) => Response::Ok {
                    data: ResponseData::SavedSearchData { search },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::DeleteSavedSearch { name } => {
            match state.store.delete_saved_search_by_name(name).await {
                Ok(true) => Response::Ok {
                    data: ResponseData::Ack,
                },
                Ok(false) => Response::Error {
                    message: format!("Saved search '{}' not found", name),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::RunSavedSearch { name, limit } => {
            match state.store.get_saved_search_by_name(name).await {
                Ok(Some(saved)) => {
                    let search = state.search.lock().await;
                    match search.search(&saved.query, *limit as usize) {
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
                Ok(None) => Response::Error {
                    message: format!("Saved search '{}' not found", name),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetStatus => {
            let total = state
                .store
                .count_messages_by_account(state.provider.account_id())
                .await
                .unwrap_or(0);
            let account_name = state
                .store
                .get_account(state.provider.account_id())
                .await
                .ok()
                .flatten()
                .map(|a| a.name)
                .unwrap_or_else(|| "unknown".to_string());
            Response::Ok {
                data: ResponseData::Status {
                    uptime_secs: state.uptime_secs(),
                    accounts: vec![account_name],
                    total_messages: total,
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

        Request::Mutation(cmd) => {
            let message_ids = match cmd {
                MutationCommand::Archive { message_ids }
                | MutationCommand::Trash { message_ids }
                | MutationCommand::Spam { message_ids }
                | MutationCommand::Star { message_ids, .. }
                | MutationCommand::SetRead { message_ids, .. }
                | MutationCommand::ModifyLabels { message_ids, .. }
                | MutationCommand::Move { message_ids, .. } => message_ids,
            };

            for msg_id in message_ids {
                let envelope = match state.store.get_envelope(msg_id).await {
                    Ok(Some(env)) => env,
                    Ok(None) => {
                        return Response::Error {
                            message: format!("Message not found: {}", msg_id),
                        };
                    }
                    Err(e) => {
                        return Response::Error {
                            message: e.to_string(),
                        };
                    }
                };
                let provider_id = &envelope.provider_id;

                let result = match cmd {
                    MutationCommand::Archive { .. } => {
                        if let Err(e) = state
                            .provider
                            .modify_labels(provider_id, &[], &["INBOX".to_string()])
                            .await
                        {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        // Remove INBOX label locally
                        let mut label_ids = state
                            .store
                            .get_message_label_ids(msg_id)
                            .await
                            .unwrap_or_default();
                        label_ids.retain(|l| l.as_str() != "INBOX");
                        state.store.set_message_labels(msg_id, &label_ids).await
                    }
                    MutationCommand::Trash { .. } => {
                        if let Err(e) = state.provider.trash(provider_id).await {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        Ok(())
                    }
                    MutationCommand::Spam { .. } => {
                        if let Err(e) = state
                            .provider
                            .modify_labels(
                                provider_id,
                                &["SPAM".to_string()],
                                &["INBOX".to_string()],
                            )
                            .await
                        {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        Ok(())
                    }
                    MutationCommand::Star { starred, .. } => {
                        if let Err(e) =
                            state.provider.set_starred(provider_id, *starred).await
                        {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        state.store.set_starred(msg_id, *starred).await
                    }
                    MutationCommand::SetRead { read, .. } => {
                        if let Err(e) =
                            state.provider.set_read(provider_id, *read).await
                        {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        state.store.set_read(msg_id, *read).await
                    }
                    MutationCommand::ModifyLabels { add, remove, .. } => {
                        if let Err(e) = state
                            .provider
                            .modify_labels(provider_id, add, remove)
                            .await
                        {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        Ok(())
                    }
                    MutationCommand::Move { target_label, .. } => {
                        if let Err(e) = state
                            .provider
                            .modify_labels(
                                provider_id,
                                &[target_label.clone()],
                                &["INBOX".to_string()],
                            )
                            .await
                        {
                            return Response::Error {
                                message: e.to_string(),
                            };
                        }
                        Ok(())
                    }
                };

                if let Err(e) = result {
                    return Response::Error {
                        message: e.to_string(),
                    };
                }
            }

            Response::Ok {
                data: ResponseData::Ack,
            }
        }

        Request::Snooze {
            message_id,
            wake_at,
        } => {
            let snoozed = Snoozed {
                message_id: message_id.clone(),
                account_id: state.provider.account_id().clone(),
                snoozed_at: chrono::Utc::now(),
                wake_at: *wake_at,
                original_labels: state
                    .store
                    .get_message_label_ids(message_id)
                    .await
                    .unwrap_or_default(),
            };
            match state.store.insert_snooze(&snoozed).await {
                Ok(()) => Response::Ok {
                    data: ResponseData::Ack,
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Unsnooze { message_id } => {
            match state.store.remove_snooze(message_id).await {
                Ok(()) => Response::Ok {
                    data: ResponseData::Ack,
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::ListSnoozed => match state.store.list_snoozed().await {
            Ok(snoozed) => Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },

        Request::ListDrafts => {
            match state
                .store
                .list_drafts(state.provider.account_id())
                .await
            {
                Ok(drafts) => Response::Ok {
                    data: ResponseData::Drafts { drafts },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::PrepareReply {
            message_id,
            reply_all,
        } => {
            let envelope = match state.store.get_envelope(message_id).await {
                Ok(Some(env)) => env,
                Ok(None) => {
                    return Response::Error {
                        message: "Message not found".to_string(),
                    };
                }
                Err(e) => {
                    return Response::Error {
                        message: e.to_string(),
                    };
                }
            };

            let from = state
                .store
                .get_account(state.provider.account_id())
                .await
                .ok()
                .flatten()
                .map(|a| a.email)
                .unwrap_or_default();

            let thread_context = match state
                .sync_engine
                .fetch_body(state.provider.as_ref(), message_id)
                .await
            {
                Ok(body) => body
                    .text_plain
                    .or(body.text_html)
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };

            let cc = if *reply_all {
                envelope
                    .cc
                    .iter()
                    .map(|a| a.email.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                String::new()
            };

            let context = ReplyContext {
                in_reply_to: envelope
                    .message_id_header
                    .clone()
                    .unwrap_or_default(),
                reply_to: envelope.from.email.clone(),
                cc,
                subject: envelope.subject.clone(),
                from,
                thread_context,
            };

            Response::Ok {
                data: ResponseData::ReplyContext { context },
            }
        }

        Request::PrepareForward { message_id } => {
            let envelope = match state.store.get_envelope(message_id).await {
                Ok(Some(env)) => env,
                Ok(None) => {
                    return Response::Error {
                        message: "Message not found".to_string(),
                    };
                }
                Err(e) => {
                    return Response::Error {
                        message: e.to_string(),
                    };
                }
            };

            let from = state
                .store
                .get_account(state.provider.account_id())
                .await
                .ok()
                .flatten()
                .map(|a| a.email)
                .unwrap_or_default();

            let forwarded_content = match state
                .sync_engine
                .fetch_body(state.provider.as_ref(), message_id)
                .await
            {
                Ok(body) => body
                    .text_plain
                    .or(body.text_html)
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };

            let context = ForwardContext {
                subject: envelope.subject.clone(),
                from,
                forwarded_content,
            };

            Response::Ok {
                data: ResponseData::ForwardContext { context },
            }
        }

        Request::SendDraft { draft } => match &state.send_provider {
            Some(sender) => {
                let account = state
                    .store
                    .get_account(state.provider.account_id())
                    .await
                    .ok()
                    .flatten();
                let from = mxr_core::types::Address {
                    name: account.as_ref().map(|a| a.name.clone()),
                    email: account
                        .as_ref()
                        .map(|a| a.email.clone())
                        .unwrap_or_else(|| "user@example.com".to_string()),
                };
                match sender.send(draft, &from).await {
                    Ok(_receipt) => Response::Ok {
                        data: ResponseData::Ack,
                    },
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                }
            }
            None => Response::Error {
                message: "No send provider configured".to_string(),
            },
        },

        Request::Unsubscribe { message_id } => {
            match state.store.get_envelope(message_id).await {
                Ok(Some(envelope)) => {
                    let client = reqwest::Client::new();
                    let result =
                        crate::unsubscribe::execute_unsubscribe(&envelope.unsubscribe, &client)
                            .await;
                    match result {
                        crate::unsubscribe::UnsubscribeResult::Success(_) => Response::Ok {
                            data: ResponseData::Ack,
                        },
                        crate::unsubscribe::UnsubscribeResult::Failed(msg) => {
                            Response::Error { message: msg }
                        }
                        crate::unsubscribe::UnsubscribeResult::NoMethod => Response::Error {
                            message: "No unsubscribe method available for this message".to_string(),
                        },
                    }
                }
                Ok(None) => Response::Error {
                    message: "Message not found".to_string(),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::SetFlags { message_id, flags } => {
            match state.store.update_flags(message_id, *flags).await {
                Ok(()) => Response::Ok {
                    data: ResponseData::Ack,
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetSyncStatus { .. } => Response::Ok {
            data: ResponseData::SyncStatus {
                last_sync: None,
                status: "ok".to_string(),
            },
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

    #[tokio::test]
    async fn dispatch_list_envelopes_by_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
            .unwrap();

        // Get labels first
        let labels_msg = IpcMessage {
            id: 10,
            payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
        };
        let resp = handle_request(&state, &labels_msg).await;
        let labels = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => labels,
            other => panic!("Expected Labels, got {:?}", other),
        };

        // Find Inbox label
        let inbox = labels
            .iter()
            .find(|l| l.name == "Inbox")
            .expect("Inbox label missing");

        // Fetch envelopes by Inbox label
        let msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: Some(inbox.id.clone()),
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
                assert!(
                    !envelopes.is_empty(),
                    "Inbox label should have envelopes, got 0. Inbox label_id={}",
                    inbox.id
                );
            }
            IpcPayload::Response(Response::Error { message }) => {
                panic!("Got error response: {message}");
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_count_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::Count {
                query: "deployment".to_string(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Count { count },
            }) => {
                assert!(count > 0, "Expected non-zero count for 'deployment'");
            }
            other => panic!("Expected Count, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_saved_searches_empty() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert!(searches.is_empty());
            }
            other => panic!("Expected empty SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_and_list_saved_searches() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Create
        let create_msg = IpcMessage {
            id: 5,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Important".to_string(),
                query: "is:starred".to_string(),
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.name, "Important");
                assert_eq!(search.query, "is:starred");
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        // List
        let list_msg = IpcMessage {
            id: 6,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert_eq!(searches.len(), 1);
                assert_eq!(searches[0].name, "Important");
            }
            other => panic!("Expected SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_status() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 7,
            payload: IpcPayload::Request(Request::GetStatus),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs: _,
                        accounts,
                        total_messages: _,
                    },
            }) => {
                assert!(!accounts.is_empty());
            }
            other => panic!("Expected Status, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_search_returns_results() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync first so search index is populated
        state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 10,
            payload: IpcPayload::Request(Request::Search {
                query: "deployment".to_string(),
                limit: 10,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { results },
            }) => {
                assert!(
                    !results.is_empty(),
                    "Search for 'deployment' should return results"
                );
            }
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_body_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
            .unwrap();

        // Get first envelope
        let envelopes_msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &envelopes_msg).await;
        let message_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert!(!envelopes.is_empty());
                envelopes[0].id.clone()
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        // Get body for that envelope
        let body_msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: message_id.clone(),
            }),
        };
        let resp = handle_request(&state, &body_msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert!(
                    body.text_plain.is_some(),
                    "Body should have text_plain content"
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }
    }

    /// Helper: sync, list envelopes, return first envelope's id.
    async fn sync_and_get_first_id(state: &Arc<AppState>) -> mxr_core::MessageId {
        state
            .sync_engine
            .sync_account(state.provider.as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 100,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert!(!envelopes.is_empty());
                envelopes[0].id.clone()
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_star() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Star {
                message_ids: vec![id.clone()],
                starred: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify flag is set
        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope {
                message_id: id,
            }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert!(
                    envelope.flags.contains(mxr_core::types::MessageFlags::STARRED),
                    "Expected STARRED flag to be set, got {:?}",
                    envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_set_read() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::SetRead {
                message_ids: vec![id.clone()],
                read: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope {
                message_id: id,
            }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert!(
                    envelope.flags.contains(mxr_core::types::MessageFlags::READ),
                    "Expected READ flag to be set, got {:?}",
                    envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_archive() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Archive {
                message_ids: vec![id],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_trash() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Trash {
                message_ids: vec![id],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Fetch body first so it's cached
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(!context.reply_to.is_empty(), "reply_to should be non-empty");
                assert!(!context.subject.is_empty(), "subject should be non-empty");
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply_all() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Fetch body first
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(!context.reply_to.is_empty(), "reply_to should be non-empty");
                assert!(!context.subject.is_empty(), "subject should be non-empty");
                // cc may or may not be empty depending on the message, but the field should exist
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_forward() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Fetch body first
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareForward {
                message_id: id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ForwardContext { context },
            }) => {
                assert!(!context.subject.is_empty(), "subject should be non-empty");
                assert!(
                    !context.forwarded_content.is_empty(),
                    "forwarded_content should be non-empty"
                );
            }
            other => panic!("Expected ForwardContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_send_draft() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: state.provider.account_id().clone(),
            in_reply_to: None,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test subject".to_string(),
            body_markdown: "Test body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft { draft }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_snooze_and_list() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Snooze
        let wake_at = chrono::Utc::now() + chrono::Duration::hours(24);
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: id.clone(),
                wake_at,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Snooze, got {:?}", other),
        }

        // List snoozed - should have 1
        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListSnoozed),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            }) => {
                assert_eq!(snoozed.len(), 1, "Expected 1 snoozed message");
            }
            other => panic!("Expected SnoozedMessages, got {:?}", other),
        }

        // Unsnooze
        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::Unsnooze {
                message_id: id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Unsnooze, got {:?}", other),
        }

        // List snoozed - should have 0
        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ListSnoozed),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            }) => {
                assert_eq!(snoozed.len(), 0, "Expected 0 snoozed messages after unsnooze");
            }
            other => panic!("Expected SnoozedMessages, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_set_flags() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        use mxr_core::types::MessageFlags;
        let flags = MessageFlags::READ | MessageFlags::STARRED;
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SetFlags {
                message_id: id.clone(),
                flags,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify flags
        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope {
                message_id: id,
            }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert_eq!(
                    envelope.flags, flags,
                    "Expected flags {:?}, got {:?}",
                    flags, envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_unsubscribe_no_method() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // The first envelope from FakeProvider fixtures uses UnsubscribeMethod::None
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Unsubscribe {
                message_id: id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message }) => {
                assert!(
                    message.contains("unsubscribe"),
                    "Expected error about unsubscribe, got: {}",
                    message
                );
            }
            other => panic!("Expected Error for no unsubscribe method, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_nonexistent_message() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let fake_id = mxr_core::MessageId::new();
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Star {
                message_ids: vec![fake_id],
                starred: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message }) => {
                assert!(
                    message.contains("not found") || message.contains("Not found"),
                    "Expected 'not found' error, got: {}",
                    message
                );
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_drafts_empty() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListDrafts),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Drafts { drafts },
            }) => {
                assert!(drafts.is_empty(), "Expected empty drafts list");
            }
            other => panic!("Expected Drafts, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_saved_search_delete() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Create a saved search
        let create_msg = IpcMessage {
            id: 20,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "ToDelete".to_string(),
                query: "is:unread".to_string(),
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.name, "ToDelete");
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        // Verify it's in the list
        let list_msg = IpcMessage {
            id: 21,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert_eq!(searches.len(), 1);
                assert_eq!(searches[0].name, "ToDelete");
            }
            other => panic!("Expected SavedSearches with 1 item, got {:?}", other),
        }

        // Delete it
        let delete_msg = IpcMessage {
            id: 22,
            payload: IpcPayload::Request(Request::DeleteSavedSearch {
                name: "ToDelete".to_string(),
            }),
        };
        let resp = handle_request(&state, &delete_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify it's gone
        let list_msg2 = IpcMessage {
            id: 23,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg2).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert!(
                    searches.is_empty(),
                    "Saved searches should be empty after delete"
                );
            }
            other => panic!("Expected empty SavedSearches, got {:?}", other),
        }
    }
}
