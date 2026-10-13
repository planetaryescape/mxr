//! Bounded WebSocket-to-byte-stream adapter; IPC framing belongs to protocol.
use crate::{BoxedIo, Connector, Result, TransportError};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

const LIMIT: usize = 16 * 1024 * 1024 + 4;

/// Accept only an HTTPS origin, with no credential-bearing URL components.
pub fn https_origin(value: &str) -> Result<String> {
    let url = url::Url::parse(value)
        .map_err(|_| TransportError::Addr("Expected an HTTPS origin".into()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(TransportError::Addr(
            "Expected an HTTPS origin without credentials, path, query or fragment".into(),
        ));
    }
    Ok(url.origin().ascii_serialization())
}

pub struct WssConnector {
    origin: String,
    proof: String,
    #[cfg(test)]
    tls: Option<tokio_tungstenite::Connector>,
}
impl WssConnector {
    pub fn new(origin: String, client_id: &str, secret: &str) -> Result<Self> {
        let origin = https_origin(&origin)?;
        Ok(Self {
            origin,
            proof: format!("{client_id}:{secret}"),
            #[cfg(test)]
            tls: None,
        })
    }
}
#[async_trait]
impl Connector for WssConnector {
    async fn connect(&self) -> Result<BoxedIo> {
        let endpoint = format!("wss{}/api/v1/remote/ipc", &self.origin[5..]);
        let error = || TransportError::Connect {
            endpoint: self.origin.clone(),
            source: io::Error::other("HTTPS/WebSocket connection or authentication failed"),
        };
        let mut request = endpoint.into_client_request().map_err(|_| error())?;
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", self.proof)
                .parse()
                .map_err(|_| error())?,
        );
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(LIMIT))
            .max_frame_size(Some(LIMIT));
        // This API rejects non-101 responses; it never forwards credentials through redirects.
        #[cfg(test)]
        let tls = self.tls.clone();
        #[cfg(not(test))]
        let tls = None;
        let (mut ws, _) =
            tokio_tungstenite::connect_async_tls_with_config(request, Some(config), false, tls)
                .await
                .map_err(|_| error())?;
        let (client, mut bridge) = tokio::io::duplex(64 * 1024);
        let task = tokio::spawn(async move {
            let mut outgoing = vec![0; 16 * 1024];
            loop {
                tokio::select! {
                    data = bridge.read(&mut outgoing) => {
                        let Ok(size) = data else {break;};
                        if size == 0 {break;}
                        if ws.send(Message::Binary(outgoing[..size].to_vec().into())).await.is_err() {break;}
                    }
                    message = ws.next() => {
                        match message {
                            Some(Ok(Message::Binary(data))) => {if bridge.write_all(&data).await.is_err() {break;} }
                            Some(Ok(Message::Ping(data))) => {if ws.send(Message::Pong(data)).await.is_err() {break;} }
                            Some(Ok(Message::Pong(_))) => (),
                            _ => break,
                        }
                    }
                }
            }
        });
        Ok(Box::new(OwnedStream {
            stream: client,
            task,
        }))
    }
    fn describe(&self) -> String {
        self.origin.clone()
    }
}
struct OwnedStream {
    stream: tokio::io::DuplexStream,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for OwnedStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl AsyncRead for OwnedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for OwnedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
#[path = "wss_tests.rs"]
mod tests;
