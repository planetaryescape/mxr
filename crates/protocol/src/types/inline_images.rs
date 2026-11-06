//! Bounded inline-image transfer; the host cache path never crosses IPC.
use serde::{Deserialize, Serialize};

pub const MAX_INLINE_IMAGE_BYTES: u64 = 15 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InlineImageChunkData {
    pub offset: u64,
    pub total_length: u64,
    pub mime_type: String,
    pub data_base64: String,
}
