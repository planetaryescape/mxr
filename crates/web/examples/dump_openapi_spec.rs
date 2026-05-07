//! Print the bridge's OpenAPI 3.1 spec to stdout.
//!
//! Used by:
//! - CI's openapi-conformance workflow (Schemathesis spec linting)
//! - Desktop app codegen (`pnpm gen:types`)
//! - Anyone wanting the spec without booting a daemon
//!
//! Run:
//!   cargo run --example dump_openapi_spec -p mxr-web > spec.json

use mxr_web::ApiDoc;
use utoipa::OpenApi;

fn main() {
    let doc = ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&doc).expect("spec serializes");
    println!("{json}");
}
