mod codec;
mod types;

pub use codec::IpcCodec;
pub use types::*;

pub const IPC_PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use mxr_core::id::*;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn request_serde_roundtrip() {
        let variants: Vec<Request> = vec![
            Request::Ping,
            Request::Shutdown,
            Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 50,
                offset: 0,
            },
            Request::GetEnvelope {
                message_id: MessageId::new(),
            },
            Request::Search {
                query: "test".to_string(),
                limit: 10,
                mode: None,
                explain: false,
            },
        ];

        for req in variants {
            let msg = IpcMessage {
                id: 1,
                payload: IpcPayload::Request(req),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.id, 1);
        }
    }

    #[test]
    fn response_serde_roundtrip() {
        let ok = Response::Ok {
            data: ResponseData::Pong,
        };
        let err = Response::Error {
            message: "something failed".to_string(),
        };

        for resp in [ok, err] {
            let msg = IpcMessage {
                id: 2,
                payload: IpcPayload::Response(resp),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.id, 2);
        }
    }

    #[test]
    fn daemon_event_roundtrip() {
        let events: Vec<DaemonEvent> = vec![
            DaemonEvent::SyncCompleted {
                account_id: AccountId::new(),
                messages_synced: 10,
            },
            DaemonEvent::SyncError {
                account_id: AccountId::new(),
                error: "timeout".to_string(),
            },
            DaemonEvent::MessageUnsnoozed {
                message_id: MessageId::new(),
            },
        ];

        for event in events {
            let msg = IpcMessage {
                id: 0,
                payload: IpcPayload::Event(event),
            };
            let json = serde_json::to_string(&msg).unwrap();
            let _parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn codec_encode_decode() {
        let mut codec = IpcCodec::new();
        let msg = IpcMessage {
            id: 42,
            payload: IpcPayload::Request(Request::Ping),
        };

        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.id, 42);
    }

    #[test]
    fn codec_multiple_messages() {
        let mut codec = IpcCodec::new();
        let mut buf = BytesMut::new();

        for i in 0..3 {
            let msg = IpcMessage {
                id: i,
                payload: IpcPayload::Request(Request::Ping),
            };
            codec.encode(msg, &mut buf).unwrap();
        }

        for i in 0..3 {
            let decoded = codec.decode(&mut buf).unwrap().unwrap();
            assert_eq!(decoded.id, i);
        }

        // No more messages
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn legacy_status_response_defaults_new_fields() {
        let json = serde_json::json!({
            "id": 7,
            "payload": {
                "type": "Response",
                "status": "Ok",
                "data": {
                    "kind": "Status",
                    "uptime_secs": 42,
                    "accounts": ["personal"],
                    "total_messages": 123,
                    "daemon_pid": 999
                }
            }
        });

        let parsed: IpcMessage = serde_json::from_value(json).unwrap();
        match parsed.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::Status {
                        sync_statuses,
                        protocol_version,
                        daemon_version,
                        daemon_build_id,
                        repair_required,
                        ..
                    },
            }) => {
                assert!(sync_statuses.is_empty());
                assert_eq!(protocol_version, 0);
                assert!(daemon_version.is_none());
                assert!(daemon_build_id.is_none());
                assert!(!repair_required);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }
}
