use super::*;
use base64::Engine as _;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct OwnedResource {
    metadata: mxr_core::DraftAttachment,
    bytes: Vec<u8>,
    ready: bool,
}

/// The generic IPC fallback acknowledges chunks without retaining their bytes.
#[derive(Clone)]
pub(super) struct DraftAttachmentBytes {
    resources: Arc<Mutex<HashMap<mxr_core::DraftAttachmentId, OwnedResource>>>,
    account_id: AccountId,
    expected_bytes: &'static [u8],
    expected_sha256: &'static str,
}

impl DraftAttachmentBytes {
    pub(super) fn new(account_id: AccountId, bytes: &'static [u8], sha256: &'static str) -> Self {
        Self {
            resources: Arc::new(Mutex::new(HashMap::new())),
            account_id,
            expected_bytes: bytes,
            expected_sha256: sha256,
        }
    }

    pub(super) fn respond(&self, request: &Request) -> Option<Response> {
        let data = match request {
            Request::BeginDraftAttachment { metadata } => {
                assert_eq!(metadata.account_id, self.account_id);
                assert_eq!(metadata.length_bytes, self.expected_bytes.len() as u64);
                assert_eq!(metadata.sha256, self.expected_sha256);
                let previous = self.resources.lock().expect("resource store").insert(
                    metadata.id.clone(),
                    OwnedResource {
                        metadata: metadata.clone(),
                        bytes: Vec::new(),
                        ready: false,
                    },
                );
                assert!(previous.is_none());
                ResponseData::DraftAttachment {
                    metadata: metadata.clone(),
                }
            }
            Request::WriteDraftAttachmentChunk {
                draft_id,
                account_id,
                resource_id,
                offset,
                data_base64,
            } => {
                let mut resources = self.resources.lock().expect("resource store");
                let resource = resources.get_mut(resource_id).expect("declared resource");
                assert_eq!(draft_id, &resource.metadata.draft_id);
                assert_eq!(account_id, &resource.metadata.account_id);
                assert!(!resource.ready);
                assert_eq!(*offset, resource.bytes.len() as u64);
                resource.bytes.extend(
                    base64::engine::general_purpose::STANDARD
                        .decode(data_base64)
                        .expect("valid chunk"),
                );
                ResponseData::Ack
            }
            Request::FinalizeDraftAttachment {
                draft_id,
                account_id,
                resource_id,
            } => {
                let mut resources = self.resources.lock().expect("resource store");
                let resource = resources.get_mut(resource_id).expect("declared resource");
                assert_eq!(draft_id, &resource.metadata.draft_id);
                assert_eq!(account_id, &resource.metadata.account_id);
                assert_eq!(resource.bytes, self.expected_bytes);
                assert_eq!(resource.metadata.length_bytes, resource.bytes.len() as u64);
                assert_eq!(resource.metadata.sha256, self.expected_sha256);
                resource.ready = true;
                ResponseData::DraftAttachment {
                    metadata: resource.metadata.clone(),
                }
            }
            _ => return None,
        };
        Some(Response::Ok { data })
    }

    pub(super) fn len(&self) -> usize {
        self.resources.lock().expect("resource store").len()
    }

    pub(super) fn assert_owned(&self, draft: &Draft) {
        let managed = draft
            .managed_attachments
            .as_ref()
            .expect("managed attachment set");
        assert!(!managed.is_empty());
        let resources = self.resources.lock().expect("resource store");
        for metadata in managed {
            assert_eq!(metadata.draft_id, draft.id);
            assert_eq!(metadata.account_id, draft.account_id);
            assert_eq!(metadata.length_bytes, self.expected_bytes.len() as u64);
            assert_eq!(metadata.sha256, self.expected_sha256);
            let resource = resources
                .get(&metadata.id)
                .expect("canonical resource bytes");
            assert_eq!(&resource.metadata, metadata);
            assert!(resource.ready);
            assert_eq!(resource.bytes, self.expected_bytes);
        }
    }
}
