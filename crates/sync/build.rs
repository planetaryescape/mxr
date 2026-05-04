// When this crate is built outside its workspace (e.g. installed from
// crates.io), the workspace `.cargo/config.toml` that sets `SQLX_OFFLINE=true`
// is not present in the published tarball and there is no DATABASE_URL.
// Default to offline mode so sqlx-macros use the bundled `.sqlx/` query cache.
//
// If the caller explicitly sets DATABASE_URL or SQLX_OFFLINE, respect that —
// `cargo sqlx prepare` needs a live DB connection and toggles these vars.
fn main() {
    println!("cargo:rerun-if-env-changed=DATABASE_URL");
    println!("cargo:rerun-if-env-changed=SQLX_OFFLINE");
    let database_url_set = std::env::var("DATABASE_URL")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let sqlx_offline_set = std::env::var("SQLX_OFFLINE").is_ok();
    if !database_url_set && !sqlx_offline_set {
        println!("cargo:rustc-env=SQLX_OFFLINE=true");
    }
}
