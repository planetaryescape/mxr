//! Sends the assistant pane's requests. They run in order on their own
//! queue, so a refetch can't overtake the decision it follows and a slow
//! mail request can't hold up the pane.

use crate::app::App;
use crate::async_result::AsyncResult;
use crate::ipc::{ipc_call, IpcRequest};
use crate::runtime::{submit_task, AsyncResultTask};
use mxr_core::MxrError;
use mxr_protocol::Response;
use tokio::sync::mpsc;

pub(crate) fn spawn_assistant_requests(
    app: &mut App,
    bg: &mpsc::UnboundedSender<IpcRequest>,
    queue: &mpsc::UnboundedSender<AsyncResultTask>,
) {
    for outbound in app.assistant.take_outbound() {
        let bg = bg.clone();
        let _ = submit_task(queue, async move {
            let result = match ipc_call(&bg, outbound.request).await {
                Ok(Response::Ok { data }) => Ok(data),
                Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
                Err(error) => Err(error),
            };
            AsyncResult::Assistant {
                call: outbound.call,
                result: Box::new(result),
            }
        });
    }
}
