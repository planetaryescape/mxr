use crate::ipc::{ipc_call, IpcRequest};
use crate::AsyncResult;
use image::DynamicImage;
use mxr_core::id::MessageId;
use mxr_core::types::HtmlImageAsset;
use mxr_core::MxrError;
use mxr_protocol::{Request, Response, ResponseData};
use ratatui::layout::Rect;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::thread::{ResizeRequest, ThreadProtocol};
use ratatui_image::Resize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const HTML_IMAGE_ASSET_CONCURRENCY: usize = 4;
const HTML_IMAGE_DECODE_CONCURRENCY: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HtmlImageKey {
    pub message_id: MessageId,
    pub source: String,
}

pub struct HtmlImageEntry {
    pub asset: HtmlImageAsset,
    pub render: HtmlImageRenderState,
}

pub(crate) struct HtmlImageAssetRequest {
    pub(crate) message_id: MessageId,
    pub(crate) allow_remote: bool,
}

pub(crate) struct HtmlImageDecodeRequest {
    pub(crate) key: HtmlImageKey,
    pub(crate) path: PathBuf,
}

pub enum HtmlImageRenderState {
    Pending,
    Ready(Box<ThreadProtocol>),
    Failed(String),
}

pub struct TerminalImageSupport {
    picker: Picker,
    protocol_type: ProtocolType,
}

impl TerminalImageSupport {
    pub fn detect() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let protocol_type = picker.protocol_type();
        Self {
            picker,
            protocol_type,
        }
    }

    pub fn protocol_name(&self) -> &'static str {
        match self.protocol_type {
            ProtocolType::Halfblocks => "halfblocks",
            ProtocolType::Sixel => "sixel",
            ProtocolType::Kitty => "kitty",
            ProtocolType::Iterm2 => "iterm2",
        }
    }

    pub(crate) fn build_protocol(
        &self,
        image: DynamicImage,
        key: HtmlImageKey,
        result_tx: mpsc::UnboundedSender<AsyncResult>,
    ) -> ThreadProtocol {
        let protocol = self.picker.new_resize_protocol(image);
        let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<ResizeRequest>();
        tokio::spawn(async move {
            while let Some(request) = resize_rx.recv().await {
                let result = request
                    .resize_encode()
                    .map_err(|error| MxrError::Ipc(error.to_string()));
                let _ = result_tx.send(AsyncResult::HtmlImageResized {
                    key: key.clone(),
                    result,
                });
            }
        });
        ThreadProtocol::new(resize_tx, Some(protocol))
    }
}

impl HtmlImageEntry {
    pub fn new(asset: HtmlImageAsset) -> Self {
        Self {
            asset,
            render: HtmlImageRenderState::Pending,
        }
    }

    pub fn ready_protocol_mut(&mut self) -> Option<&mut ThreadProtocol> {
        match &mut self.render {
            HtmlImageRenderState::Ready(protocol) => Some(protocol.as_mut()),
            HtmlImageRenderState::Pending | HtmlImageRenderState::Failed(_) => None,
        }
    }

    pub fn height_for(&self, width: u16, max_height: u16) -> u16 {
        if width == 0 || max_height == 0 {
            return 0;
        }
        match &self.render {
            HtmlImageRenderState::Ready(protocol) => protocol
                .as_ref()
                .size_for(Resize::Fit(None), Rect::new(0, 0, width, max_height))
                .map(|size| size.height.max(1))
                .unwrap_or_else(|| self.placeholder_height()),
            HtmlImageRenderState::Pending | HtmlImageRenderState::Failed(_) => {
                self.placeholder_height()
            }
        }
    }

    pub fn placeholder_height(&self) -> u16 {
        3
    }
}

pub(crate) fn spawn_html_image_asset_worker(
    bg: mpsc::UnboundedSender<IpcRequest>,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<HtmlImageAssetRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<HtmlImageAssetRequest>();
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(HTML_IMAGE_ASSET_CONCURRENCY));
        let mut join_set = JoinSet::new();
        let mut input_closed = false;

        loop {
            if input_closed && join_set.is_empty() {
                break;
            }

            tokio::select! {
                maybe_request = rx.recv(), if !input_closed => {
                    match maybe_request {
                        Some(request) => {
                            let Ok(permit) = semaphore.clone().acquire_owned().await else {
                                break;
                            };
                            let bg = bg.clone();
                            join_set.spawn(async move {
                                let _permit = permit;
                                let resp = ipc_call(
                                    &bg,
                                    Request::GetHtmlImageAssets {
                                        message_id: request.message_id.clone(),
                                        allow_remote: request.allow_remote,
                                    },
                                )
                                .await;
                                let result = match resp {
                                    Ok(Response::Ok {
                                        data: ResponseData::HtmlImageAssets { assets, .. },
                                    }) => Ok(assets),
                                    Ok(Response::Error { message }) => Err(MxrError::Ipc(message)),
                                    Err(error) => Err(error),
                                    _ => Err(MxrError::Ipc("unexpected response".into())),
                                };
                                AsyncResult::HtmlImageAssets {
                                    message_id: request.message_id,
                                    allow_remote: request.allow_remote,
                                    result,
                                }
                            });
                        }
                        None => input_closed = true,
                    }
                }
                joined = join_set.join_next(), if !join_set.is_empty() => {
                    if let Some(Ok(result)) = joined {
                        let _ = result_tx.send(result);
                    }
                }
            }
        }
    });
    tx
}

pub(crate) fn spawn_html_image_decode_worker(
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<HtmlImageDecodeRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<HtmlImageDecodeRequest>();
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(HTML_IMAGE_DECODE_CONCURRENCY));
        let mut join_set = JoinSet::new();
        let mut input_closed = false;

        loop {
            if input_closed && join_set.is_empty() {
                break;
            }

            tokio::select! {
                maybe_request = rx.recv(), if !input_closed => {
                    match maybe_request {
                        Some(request) => {
                            let Ok(permit) = semaphore.clone().acquire_owned().await else {
                                break;
                            };
                            join_set.spawn(async move {
                                let _permit = permit;
                                let key = request.key;
                                let path = request.path;
                                let result = tokio::task::spawn_blocking(move || decode_image(&path))
                                    .await
                                    .map_err(|error| MxrError::Ipc(error.to_string()))
                                    .and_then(|result| result);
                                AsyncResult::HtmlImageDecoded { key, result }
                            });
                        }
                        None => input_closed = true,
                    }
                }
                joined = join_set.join_next(), if !join_set.is_empty() => {
                    if let Some(Ok(result)) = joined {
                        let _ = result_tx.send(result);
                    }
                }
            }
        }
    });
    tx
}

fn decode_image(path: &PathBuf) -> Result<DynamicImage, MxrError> {
    let reader =
        image::ImageReader::open(path).map_err(|error| MxrError::Ipc(error.to_string()))?;
    let reader = reader
        .with_guessed_format()
        .map_err(|error| MxrError::Ipc(error.to_string()))?;
    reader
        .decode()
        .map_err(|error| MxrError::Ipc(error.to_string()))
}
