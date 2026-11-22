//! Slice 1 quality bar: every top-level IPC payload type registers as an
//! OpenAPI schema so utoipa-axum can collect them automatically when the
//! bridge crate enables `mxr-protocol/openapi`.
//!
//! Hits a representative payload from each enum (Request, Response,
//! DaemonEvent, MutationCommand, ResponseData) plus core types that travel
//! across the wire — not just `Ping`.

#![cfg(feature = "openapi")]

use mxr_protocol::{
    DaemonEvent, IpcMessage, IpcPayload, MutationCommand, Request, Response, ResponseData,
};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(
    IpcMessage,
    IpcPayload,
    Request,
    Response,
    ResponseData,
    DaemonEvent,
    MutationCommand,
)))]
struct ProtocolDoc;

#[test]
fn protocol_top_level_types_register_in_openapi() {
    let doc = ProtocolDoc::openapi();
    let json = serde_json::to_value(&doc).expect("doc serializes");
    let schemas = json
        .pointer("/components/schemas")
        .and_then(|v| v.as_object())
        .expect("schemas registered");

    for required in [
        "IpcMessage",
        "IpcPayload",
        "Request",
        "Response",
        "ResponseData",
        "DaemonEvent",
        "MutationCommand",
    ] {
        assert!(
            schemas.contains_key(required),
            "schema {required} missing from OpenAPI components"
        );
    }
}

#[test]
fn request_schema_uses_serde_tag() {
    let doc = ProtocolDoc::openapi();
    let json = serde_json::to_value(&doc).expect("doc serializes");
    let request_schema = json
        .pointer("/components/schemas/Request")
        .expect("Request schema present");

    let serialized = serde_json::to_string(request_schema).expect("schema serializes");
    assert!(
        serialized.contains("\"cmd\""),
        "Request schema must surface the serde discriminator field `cmd`; got {serialized}"
    );
}
