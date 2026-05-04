#![allow(clippy::unwrap_used)]

use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_manifest(path: &str) -> toml::Value {
    let manifest = fs::read_to_string(repo_root().join(path)).unwrap();
    toml::from_str(&manifest).unwrap()
}

#[test]
fn provider_crates_do_not_depend_on_compose() {
    let crates_dir = repo_root().join("crates");
    for entry in fs::read_dir(crates_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("provider-") {
            continue;
        }

        let manifest_path = format!("crates/{name}/Cargo.toml");
        let manifest = read_manifest(&manifest_path);
        let dependencies = manifest
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .unwrap();

        assert!(
            !dependencies.contains_key("mxr-compose"),
            "{manifest_path} still depends on mxr-compose"
        );
    }
}

#[test]
fn provider_send_crates_depend_on_outbound() {
    for manifest_path in [
        "crates/provider-gmail/Cargo.toml",
        "crates/provider-smtp/Cargo.toml",
    ] {
        let manifest = read_manifest(manifest_path);
        let dependencies = manifest
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .unwrap();

        assert!(
            dependencies.contains_key("mxr-outbound"),
            "{manifest_path} should depend on mxr-outbound"
        );
    }
}

#[test]
fn vendored_async_imap_is_not_a_workspace_member() {
    assert!(
        !repo_root().join("vendor/async-imap").exists(),
        "vendor/async-imap should not remain in the repo once registry dependency is in use"
    );
}

#[test]
fn root_package_is_publishable_and_uses_registry_async_imap() {
    let manifest = read_manifest("Cargo.toml");
    let package = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .unwrap();
    // Root mxr is published to crates.io as of v0.5.0; an explicit
    // `publish = false` would silently break the release pipeline.
    assert_ne!(
        package.get("publish").and_then(toml::Value::as_bool),
        Some(false),
        "root mxr package must be publishable for crates.io release flow"
    );

    let async_imap = manifest
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .and_then(|dependencies| dependencies.get("async-imap"))
        .and_then(toml::Value::as_table)
        .unwrap();

    assert!(
        !async_imap.contains_key("path"),
        "workspace async-imap dependency should come from the registry, not vendor/"
    );
    assert_eq!(
        async_imap.get("package").and_then(toml::Value::as_str),
        Some("mxr-async-imap")
    );

    let include = package
        .get("include")
        .and_then(toml::Value::as_array)
        .unwrap();
    assert!(
        !include
            .iter()
            .filter_map(toml::Value::as_str)
            .any(|item| item == "vendor/**"),
        "root package should not include vendor/** now that async-imap is no longer vendored"
    );
}
