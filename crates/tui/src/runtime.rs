use crate::app;
use crate::async_result::{AsyncResult, SearchResultData, StatusSnapshot};
use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::MxrError;
use mxr_protocol::{Request, Response, ResponseData};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio::time::Instant;

pub(crate) type AsyncResultTask = Pin<Box<dyn Future<Output = AsyncResult> + Send + 'static>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplaceableRequestKey {
    Search(app::SearchTarget),
    SearchCount,
    Thread,
    RuleDetail,
    RuleHistory,
    RuleForm,
    Status,
    DiagnosticsStatus,
    DiagnosticsDoctor,
    DiagnosticsEvents,
    DiagnosticsLogs,
}

#[derive(Debug)]
pub(crate) enum ReplaceableRequest {
    Search(app::PendingSearchRequest),
    SearchCount(app::PendingSearchCountRequest),
    Thread {
        thread_id: mxr_core::ThreadId,
        request_id: u64,
        enqueued_at: Instant,
    },
    RuleDetail {
        rule: String,
        request_id: u64,
        enqueued_at: Instant,
    },
    RuleHistory {
        rule: String,
        request_id: u64,
        enqueued_at: Instant,
    },
    RuleForm {
        rule: String,
        request_id: u64,
        enqueued_at: Instant,
    },
    Status {
        request_id: u64,
        enqueued_at: Instant,
    },
    Diagnostics {
        kind: ReplaceableRequestKey,
        request: Box<Request>,
        request_id: u64,
        enqueued_at: Instant,
    },
}

impl ReplaceableRequest {
    pub(crate) fn key(&self) -> ReplaceableRequestKey {
        match self {
            Self::Search(pending) => ReplaceableRequestKey::Search(pending.target),
            Self::SearchCount(_) => ReplaceableRequestKey::SearchCount,
            Self::Thread { .. } => ReplaceableRequestKey::Thread,
            Self::RuleDetail { .. } => ReplaceableRequestKey::RuleDetail,
            Self::RuleHistory { .. } => ReplaceableRequestKey::RuleHistory,
            Self::RuleForm { .. } => ReplaceableRequestKey::RuleForm,
            Self::Status { .. } => ReplaceableRequestKey::Status,
            Self::Diagnostics { kind, .. } => *kind,
        }
    }
}

pub(crate) fn spawn_replaceable_request_worker(
    bg: mpsc::UnboundedSender<IpcRequest>,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<ReplaceableRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ReplaceableRequest>();
    tokio::spawn(async move {
        let mut pending = VecDeque::new();
        loop {
            if pending.is_empty() {
                let Some(request) = rx.recv().await else {
                    break;
                };
                enqueue_replaceable_request(&mut pending, request);
            }

            while let Ok(request) = rx.try_recv() {
                enqueue_replaceable_request(&mut pending, request);
            }

            let Some(request) = pending.pop_front() else {
                continue;
            };

            let result = execute_replaceable_request(&bg, request).await;
            let _ = result_tx.send(result);
        }
    });
    tx
}

pub(crate) fn spawn_task_worker(
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<AsyncResultTask> {
    let (tx, mut rx) = mpsc::unbounded_channel::<AsyncResultTask>();
    tokio::spawn(async move {
        while let Some(task) = rx.recv().await {
            let _ = result_tx.send(task.await);
        }
    });
    tx
}

pub(crate) fn submit_task<F>(
    tx: &mpsc::UnboundedSender<AsyncResultTask>,
    task: F,
) -> Result<(), mpsc::error::SendError<AsyncResultTask>>
where
    F: Future<Output = AsyncResult> + Send + 'static,
{
    tx.send(Box::pin(task))
}

pub(crate) fn enqueue_replaceable_request(
    pending: &mut VecDeque<ReplaceableRequest>,
    request: ReplaceableRequest,
) {
    let key = request.key();
    if let Some(index) = pending.iter().position(|existing| existing.key() == key) {
        pending[index] = request;
    } else {
        pending.push_back(request);
    }
}

async fn execute_replaceable_request(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    request: ReplaceableRequest,
) -> AsyncResult {
    match request {
        ReplaceableRequest::Search(pending) => {
            tracing::trace!(
                target = ?pending.target,
                session_id = pending.session_id,
                "tui replaceable request dequeued"
            );
            let query = pending.query.clone();
            let target = pending.target;
            let append = pending.append;
            let session_id = pending.session_id;
            let results = match ipc_call(
                bg,
                Request::Search {
                    query,
                    limit: pending.limit,
                    offset: pending.offset,
                    mode: Some(pending.mode),
                    sort: Some(pending.sort),
                    explain: false,
                },
            )
            .await
            {
                Ok(Response::Ok {
                    data:
                        ResponseData::SearchResults {
                            results, has_more, ..
                        },
                }) => {
                    let mut scores = std::collections::HashMap::new();
                    let message_ids = results
                        .into_iter()
                        .map(|result| {
                            scores.insert(result.message_id.clone(), result.score);
                            result.message_id
                        })
                        .collect::<Vec<_>>();
                    if message_ids.is_empty() {
                        Ok(SearchResultData {
                            envelopes: Vec::new(),
                            scores,
                            has_more,
                        })
                    } else {
                        match ipc_call(bg, Request::ListEnvelopesByIds { message_ids }).await {
                            Ok(Response::Ok {
                                data: ResponseData::Envelopes { envelopes },
                            }) => Ok(SearchResultData {
                                envelopes,
                                scores,
                                has_more,
                            }),
                            Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                            Err(error) => Err(error),
                            _ => Err(MxrError::Ipc("unexpected response".into())),
                        }
                    }
                }
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::Search {
                target,
                append,
                session_id,
                result: results,
            }
        }
        ReplaceableRequest::SearchCount(pending) => {
            tracing::trace!(session_id = pending.session_id, "tui search count dequeued");
            let session_id = pending.session_id;
            let result = match ipc_call(
                bg,
                Request::Count {
                    query: pending.query,
                    mode: Some(pending.mode),
                },
            )
            .await
            {
                Ok(Response::Ok {
                    data: ResponseData::Count { count },
                }) => Ok(count),
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::SearchCount { session_id, result }
        }
        ReplaceableRequest::Thread {
            thread_id,
            request_id,
            enqueued_at,
        } => {
            tracing::trace!(
                request_id,
                queue_wait_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0,
                "tui thread fetch dequeued"
            );
            let result = match ipc_call(
                bg,
                Request::GetThread {
                    thread_id: thread_id.clone(),
                },
            )
            .await
            {
                Ok(Response::Ok {
                    data: ResponseData::Thread { thread, messages },
                }) => Ok((thread, messages)),
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::Thread {
                thread_id,
                request_id,
                result,
            }
        }
        ReplaceableRequest::RuleDetail {
            rule,
            request_id,
            enqueued_at,
        } => {
            tracing::trace!(
                request_id,
                queue_wait_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0,
                "tui rule detail dequeued"
            );
            let result = match ipc_call(bg, Request::GetRule { rule }).await {
                Ok(Response::Ok {
                    data: ResponseData::RuleData { rule },
                }) => Ok(rule),
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::RuleDetail { request_id, result }
        }
        ReplaceableRequest::RuleHistory {
            rule,
            request_id,
            enqueued_at,
        } => {
            tracing::trace!(
                request_id,
                queue_wait_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0,
                "tui rule history dequeued"
            );
            let result = match ipc_call(
                bg,
                Request::ListRuleHistory {
                    rule: Some(rule),
                    limit: 20,
                },
            )
            .await
            {
                Ok(Response::Ok {
                    data: ResponseData::RuleHistory { entries },
                }) => Ok(entries),
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::RuleHistory { request_id, result }
        }
        ReplaceableRequest::RuleForm {
            rule,
            request_id,
            enqueued_at,
        } => {
            tracing::trace!(
                request_id,
                queue_wait_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0,
                "tui rule form dequeued"
            );
            let result = match ipc_call(bg, Request::GetRuleForm { rule }).await {
                Ok(Response::Ok {
                    data: ResponseData::RuleFormData { form },
                }) => Ok(form),
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::RuleForm { request_id, result }
        }
        ReplaceableRequest::Status {
            request_id,
            enqueued_at,
        } => {
            tracing::trace!(
                request_id,
                queue_wait_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0,
                "tui status refresh dequeued"
            );
            let result = match ipc_call(bg, Request::GetStatus).await {
                Ok(Response::Ok {
                    data:
                        ResponseData::Status {
                            uptime_secs,
                            daemon_pid,
                            accounts,
                            total_messages,
                            sync_statuses,
                            ..
                        },
                }) => Ok(StatusSnapshot {
                    uptime_secs,
                    daemon_pid,
                    accounts,
                    total_messages,
                    sync_statuses,
                }),
                Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
                _ => Err(MxrError::Ipc("unexpected response".into())),
            };
            AsyncResult::Status { request_id, result }
        }
        ReplaceableRequest::Diagnostics {
            request_id,
            request,
            enqueued_at,
            ..
        } => {
            tracing::trace!(
                request_id,
                queue_wait_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0,
                "tui diagnostics refresh dequeued"
            );
            let result = ipc_call(bg, *request).await;
            AsyncResult::Diagnostics {
                request_id,
                result: Box::new(result),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    #[test]
    fn enqueue_replaceable_request_replaces_matching_status_requests() {
        let mut pending = VecDeque::new();
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Status {
                request_id: 1,
                enqueued_at: Instant::now(),
            },
        );
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Status {
                request_id: 2,
                enqueued_at: Instant::now(),
            },
        );

        assert_eq!(pending.len(), 1);
        match pending.pop_front() {
            Some(ReplaceableRequest::Status { request_id, .. }) => assert_eq!(request_id, 2),
            other => panic!("expected status request, got {other:?}"),
        }
    }

    #[test]
    fn enqueue_replaceable_request_replaces_matching_diagnostics_requests() {
        let mut pending = VecDeque::new();
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Diagnostics {
                kind: ReplaceableRequestKey::DiagnosticsLogs,
                request: Box::new(Request::GetLogs {
                    limit: 20,
                    level: None,
                }),
                request_id: 1,
                enqueued_at: Instant::now(),
            },
        );
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Diagnostics {
                kind: ReplaceableRequestKey::DiagnosticsLogs,
                request: Box::new(Request::GetLogs {
                    limit: 50,
                    level: None,
                }),
                request_id: 2,
                enqueued_at: Instant::now(),
            },
        );

        assert_eq!(pending.len(), 1);
        match pending.pop_front() {
            Some(ReplaceableRequest::Diagnostics {
                kind,
                request,
                request_id,
                ..
            }) => {
                assert_eq!(kind, ReplaceableRequestKey::DiagnosticsLogs);
                assert_eq!(request_id, 2);
                match *request {
                    Request::GetLogs { limit, level: None } => assert_eq!(limit, 50),
                    other => panic!("expected diagnostics logs request, got {other:?}"),
                }
            }
            other => panic!("expected diagnostics logs request, got {other:?}"),
        }
    }

    #[test]
    fn enqueue_replaceable_request_keeps_distinct_search_targets() {
        let mut pending = VecDeque::new();
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Search(app::PendingSearchRequest {
                query: "alpha".into(),
                mode: mxr_core::SearchMode::Lexical,
                sort: mxr_core::types::SortOrder::DateDesc,
                limit: 20,
                offset: 0,
                target: app::SearchTarget::Mailbox,
                append: false,
                session_id: 1,
            }),
        );
        enqueue_replaceable_request(
            &mut pending,
            ReplaceableRequest::Search(app::PendingSearchRequest {
                query: "beta".into(),
                mode: mxr_core::SearchMode::Lexical,
                sort: mxr_core::types::SortOrder::DateDesc,
                limit: 20,
                offset: 0,
                target: app::SearchTarget::SearchPage,
                append: false,
                session_id: 2,
            }),
        );

        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn task_worker_runs_submitted_tasks_serially() {
        let (result_tx, mut result_rx) = mpsc::unbounded_channel();
        let worker = spawn_task_worker(result_tx);
        let started_second = Arc::new(AtomicBool::new(false));
        let (release_first_tx, release_first_rx) = oneshot::channel::<()>();

        submit_task(&worker, async move {
            let _ = release_first_rx.await;
            AsyncResult::LocalStateSaved(Ok(()))
        })
        .expect("queue first task");

        let started_second_flag = started_second.clone();
        submit_task(&worker, async move {
            started_second_flag.store(true, Ordering::SeqCst);
            AsyncResult::LocalStateSaved(Ok(()))
        })
        .expect("queue second task");

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !started_second.load(Ordering::SeqCst),
            "second task should stay queued until the first completes"
        );

        release_first_tx.send(()).expect("release first task");

        assert!(matches!(
            timeout(Duration::from_secs(1), result_rx.recv()).await,
            Ok(Some(AsyncResult::LocalStateSaved(Ok(()))))
        ));

        assert!(matches!(
            timeout(Duration::from_secs(1), result_rx.recv()).await,
            Ok(Some(AsyncResult::LocalStateSaved(Ok(()))))
        ));

        assert!(started_second.load(Ordering::SeqCst));
    }
}
