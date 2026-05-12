//! Search IPC helpers: multi-segment drain for progressive search UX.

use crate::app::{PendingSearchRequest, SearchTarget, SEARCH_PAGE_SIZE};
use crate::async_result::{AsyncResult, SearchResultData};
use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::{
    types::{SearchMode, SortOrder},
    MessageId, MxrError,
};
use mxr_protocol::{Request, Response, ResponseData};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// First daemon page for interactive search (`SearchPage`): tight limit so
/// hits paint before [`SEARCH_STREAM_SEGMENT`] drains the remainder of one
/// search-page window.
pub(crate) const SEARCH_FIRST_SEGMENT: u32 = 40;
/// Follow-up pages while filling [`SEARCH_PAGE_SIZE`].
pub(crate) const SEARCH_STREAM_SEGMENT: u32 = 88;

pub(crate) async fn ipc_search_segment(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    query: &str,
    mode: SearchMode,
    sort: SortOrder,
    limit: u32,
    offset: u32,
) -> Result<SearchResultData, MxrError> {
    match ipc_call(
        bg,
        Request::Search {
            query: query.to_owned(),
            limit,
            offset,
            mode: Some(mode),
            sort: Some(sort),
            explain: false,
        },
    )
    .await
    {
        Ok(Response::Ok {
            data: ResponseData::SearchResults {
                results, has_more, ..
            },
        }) => {
            let mut scores = HashMap::<MessageId, f32>::new();
            let message_ids: Vec<MessageId> = results
                .into_iter()
                .map(|result| {
                    scores.insert(result.message_id.clone(), result.score);
                    result.message_id
                })
                .collect();
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
                    Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                    Err(error) => Err(error),
                    _ => Err(MxrError::Ipc("unexpected response".into())),
                }
            }
        }
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(error) => Err(error),
        _ => Err(MxrError::Ipc("unexpected response".into())),
    }
}

pub(crate) async fn run_streamed_search_page_initial(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    result_tx: &mpsc::UnboundedSender<AsyncResult>,
    pending: PendingSearchRequest,
) {
    debug_assert!(pending.target == SearchTarget::SearchPage);
    debug_assert!(!pending.append);
    debug_assert_eq!(pending.offset, 0);

    let PendingSearchRequest {
        query,
        mode,
        sort,
        limit: page_cap,
        offset: start_offset,
        target,
        session_id,
        ..
    } = pending;

    let page_cap = page_cap.min(SEARCH_PAGE_SIZE);
    let mut cumulative_on_page = 0u32;
    let mut first = true;

    loop {
        let remaining = page_cap.saturating_sub(cumulative_on_page);
        if remaining == 0 {
            break;
        }
        let segment_len = if cumulative_on_page == 0 {
            SEARCH_FIRST_SEGMENT.min(remaining)
        } else {
            SEARCH_STREAM_SEGMENT.min(remaining)
        };

        let offset = start_offset.saturating_add(cumulative_on_page);
        match ipc_search_segment(bg, &query, mode, sort.clone(), segment_len, offset).await {
            Ok(data) => {
                let n = data.envelopes.len() as u32;
                let daemon_has_more = data.has_more;

                let more_same_page_expected =
                    cumulative_on_page + n < page_cap && n > 0 && daemon_has_more;
                let ui_has_more = if more_same_page_expected {
                    true
                } else {
                    daemon_has_more
                };

                let _ = result_tx.send(AsyncResult::Search {
                    target,
                    append: !first,
                    session_id,
                    result: Ok(SearchResultData {
                        envelopes: data.envelopes,
                        scores: data.scores,
                        has_more: ui_has_more,
                    }),
                });
                first = false;
                cumulative_on_page += n;

                if !daemon_has_more || n == 0 {
                    break;
                }
                if cumulative_on_page >= page_cap {
                    break;
                }
            }
            Err(error) => {
                let _ = result_tx.send(AsyncResult::Search {
                    target,
                    append: !first,
                    session_id,
                    result: Err(error),
                });
                return;
            }
        }
    }
}
