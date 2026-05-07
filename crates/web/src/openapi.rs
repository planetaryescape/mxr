//! OpenAPI 3.1 surface for the mxr HTTP bridge.
//!
//! Slice 2 — scaffolding only. Slice 3 wires the existing 47 routes through
//! `OpenApiRouter` so they show up in the generated spec. Slice 6 adds the
//! ~40 missing routes for full Request-enum parity.

use mxr_protocol::{DaemonEvent, MutationCommand, Request, Response, ResponseData};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

/// Top-level OpenAPI document. Per-route `#[utoipa::path]` annotations get
/// folded in by `OpenApiRouter` when the bridge crate's router is constructed.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "mxr HTTP Bridge",
        description = "Local-first email daemon HTTP/WebSocket surface. \
                       All routes (except /api/v1/health) require a bearer \
                       token from ~/.config/mxr/bridge-token.",
        license(name = "MIT OR Apache-2.0"),
        contact(name = "mxr", url = "https://mxr.sh")
    ),
    components(schemas(
        Request,
        Response,
        ResponseData,
        DaemonEvent,
        MutationCommand,
    )),
    modifiers(&BearerSecurity),
    security(("bearer" = []))
)]
pub struct ApiDoc;

/// Registers the bearer-token security scheme so the Swagger UI "Authorize"
/// button works and so generated SDKs know the wire format.
struct BearerSecurity;

impl Modify for BearerSecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .get_or_insert_with(utoipa::openapi::Components::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("opaque")
                    .description(Some(
                        "Token from `~/.config/mxr/bridge-token`. Send via \
                         `Authorization: Bearer <token>` header. \
                         WebSocket clients can also pass it via the \
                         `?token=<token>` query string or the \
                         `Sec-WebSocket-Protocol: bearer, <token>` subprotocol.",
                    ))
                    .build(),
            ),
        );
    }
}
