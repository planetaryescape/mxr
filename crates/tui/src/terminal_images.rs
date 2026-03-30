use crate::AsyncResult;
use image::DynamicImage;
use mxr_core::id::MessageId;
use mxr_core::types::HtmlImageAsset;
use mxr_core::MxrError;
use ratatui::layout::Rect;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::thread::{ResizeRequest, ThreadProtocol};
use ratatui_image::Resize;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HtmlImageKey {
    pub message_id: MessageId,
    pub source: String,
}

pub struct HtmlImageEntry {
    pub asset: HtmlImageAsset,
    pub render: HtmlImageRenderState,
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

pub(crate) fn spawn_image_decode(
    key: HtmlImageKey,
    path: PathBuf,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || decode_image(&path))
            .await
            .map_err(|error| MxrError::Ipc(error.to_string()))
            .and_then(|result| result);
        let _ = result_tx.send(AsyncResult::HtmlImageDecoded { key, result });
    });
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
