use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// Namespace UUID for provider ID hashing (UUID v5)
const MXR_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            pub fn as_str(&self) -> String {
                self.0.to_string()
            }

            pub fn from_provider_id(provider: &str, id: &str) -> Self {
                let input = format!("{provider}:{id}");
                Self(Uuid::new_v5(&MXR_NAMESPACE, input.as_bytes()))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

typed_id!(AccountId);
typed_id!(MessageId);
typed_id!(ThreadId);
typed_id!(LabelId);
typed_id!(DraftId);
typed_id!(AttachmentId);
typed_id!(SavedSearchId);
typed_id!(RuleId);
typed_id!(SemanticChunkId);
typed_id!(SemanticProfileId);
