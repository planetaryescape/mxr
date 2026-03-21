use futures::{SinkExt, StreamExt};
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

pub struct Client {
    framed: Framed<UnixStream, IpcCodec>,
    next_id: AtomicU64,
    event_tx: Option<mpsc::UnboundedSender<DaemonEvent>>,
}

impl Client {
    pub async fn connect(socket_path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self {
            framed: Framed::new(stream, IpcCodec::new()),
            next_id: AtomicU64::new(1),
            event_tx: None,
        })
    }

    pub fn with_event_channel(mut self, tx: mpsc::UnboundedSender<DaemonEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub async fn raw_request(&mut self, req: Request) -> Result<Response, MxrError> {
        self.request(req).await
    }

    async fn request(&mut self, req: Request) -> Result<Response, MxrError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = IpcMessage {
            id,
            payload: IpcPayload::Request(req),
        };
        self.framed
            .send(msg)
            .await
            .map_err(|e| MxrError::Ipc(e.to_string()))?;

        loop {
            match self.framed.next().await {
                Some(Ok(resp_msg)) => match resp_msg.payload {
                    IpcPayload::Response(resp) if resp_msg.id == id => return Ok(resp),
                    IpcPayload::Event(event) => {
                        if let Some(ref tx) = self.event_tx {
                            let _ = tx.send(event);
                        }
                        continue;
                    }
                    _ => continue,
                },
                Some(Err(e)) => return Err(MxrError::Ipc(describe_ipc_failure(&e.to_string()))),
                None => {
                    return Err(MxrError::Ipc(
                        "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading.".into(),
                    ))
                }
            }
        }
    }

    pub async fn list_envelopes(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, MxrError> {
        let resp = self
            .request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit,
                offset,
            })
            .await?;

        match resp {
            Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } => Ok(envelopes),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_labels(&mut self) -> Result<Vec<Label>, MxrError> {
        let resp = self
            .request(Request::ListLabels { account_id: None })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Labels { labels },
            } => Ok(labels),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn search(
        &mut self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SearchResultItem>, MxrError> {
        let resp = self
            .request(Request::Search {
                query: query.to_string(),
                limit,
                mode: None,
                explain: false,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            } => Ok(results),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_envelope(&mut self, message_id: &MessageId) -> Result<Envelope, MxrError> {
        let resp = self
            .request(Request::GetEnvelope {
                message_id: message_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Envelope { envelope },
            } => Ok(envelope),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_body(&mut self, message_id: &MessageId) -> Result<MessageBody, MxrError> {
        let resp = self
            .request(Request::GetBody {
                message_id: message_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Body { body },
            } => Ok(body),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_thread(
        &mut self,
        thread_id: &ThreadId,
    ) -> Result<(Thread, Vec<Envelope>), MxrError> {
        let resp = self
            .request(Request::GetThread {
                thread_id: thread_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Thread { thread, messages },
            } => Ok((thread, messages)),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_saved_searches(
        &mut self,
    ) -> Result<Vec<mxr_core::types::SavedSearch>, MxrError> {
        let resp = self.request(Request::ListSavedSearches).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SavedSearches { searches },
            } => Ok(searches),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_subscriptions(
        &mut self,
        limit: u32,
    ) -> Result<Vec<mxr_core::types::SubscriptionSummary>, MxrError> {
        let resp = self.request(Request::ListSubscriptions { limit }).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Subscriptions { subscriptions },
            } => Ok(subscriptions),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn ping(&mut self) -> Result<(), MxrError> {
        let resp = self.request(Request::Ping).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Pong,
            } => Ok(()),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }
}

fn describe_ipc_failure(message: &str) -> String {
    if message.contains("unknown variant") || message.contains("missing field") {
        format!("IPC protocol mismatch: {message}. Restart the daemon after upgrading.")
    } else {
        message.to_string()
    }
}
