//! Slice 1 quality bar: every top-level IPC payload type registers as an
//! OpenAPI schema so utoipa-axum can collect them automatically when the
//! bridge crate enables `mxr-protocol/openapi`.
//!
//! Hits a representative payload from each enum (Request, Response,
//! DaemonEvent, MutationCommand, ResponseData) plus core types that travel
//! across the wire — not just `Ping`.

#![cfg(feature = "openapi")]

use mxr_protocol::{DaemonEvent, IpcMessage, IpcPayload, MutationCommand, Request, Response, ResponseData};
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
    // Request is `#[serde(tag = "cmd")]` — the OpenAPI schema must reflect
    // that so generated clients know how to discriminate variants.
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

#[test]
fn response_schema_uses_serde_tag_status() {
    let doc = ProtocolDoc::openapi();
    let json = serde_json::to_value(&doc).expect("doc serializes");
    let response_schema = json
        .pointer("/components/schemas/Response")
        .expect("Response schema present");

    let serialized = serde_json::to_string(response_schema).expect("schema serializes");
    assert!(
        serialized.contains("\"status\""),
        "Response schema must surface the serde discriminator field `status`; got {serialized}"
    );
}

#[test]
fn mutation_command_schema_uses_serde_tag_mutation() {
    let doc = ProtocolDoc::openapi();
    let json = serde_json::to_value(&doc).expect("doc serializes");
    let mutation_schema = json
        .pointer("/components/schemas/MutationCommand")
        .expect("MutationCommand schema present");

    let serialized = serde_json::to_string(mutation_schema).expect("schema serializes");
    assert!(
        serialized.contains("\"mutation\""),
        "MutationCommand schema must surface the serde discriminator field `mutation`; got {serialized}"
    );
}

#[test]
fn daemon_event_schema_includes_operation_variants() {
    // DaemonEvent must include OperationStarted/Progress/Completed since the
    // bridge's WebSocket stream emits those for long-running ops (see slice 6
    // analytics rebuild + semantic reindex).
    let doc = ProtocolDoc::openapi();
    let json = serde_json::to_value(&doc).expect("doc serializes");
    let event_schema = json
        .pointer("/components/schemas/DaemonEvent")
        .expect("DaemonEvent schema present");

    let serialized = serde_json::to_string(event_schema).expect("schema serializes");
    for variant in ["OperationStarted", "OperationProgress", "OperationCompleted"] {
        assert!(
            serialized.contains(variant),
            "DaemonEvent schema must surface variant {variant}; got {serialized}"
        );
    }
}
