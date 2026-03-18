use futures::{SinkExt, StreamExt};
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

pub struct Client {
    framed: Framed<UnixStream, IpcCodec>,
    next_id: AtomicU64,
}

impl Client {
    pub async fn connect(socket_path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self {
            framed: Framed::new(stream, IpcCodec::new()),
            next_id: AtomicU64::new(1),
        })
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
                Some(Ok(resp_msg)) => {
                    if resp_msg.id == id {
                        match resp_msg.payload {
                            IpcPayload::Response(resp) => return Ok(resp),
                            _ => continue,
                        }
                    }
                }
                Some(Err(e)) => return Err(MxrError::Ipc(e.to_string())),
                None => return Err(MxrError::Ipc("Connection closed".into())),
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
