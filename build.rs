// When this crate is installed from crates.io, the workspace
// `.cargo/config.toml` that sets `SQLX_OFFLINE=true` is not present in the
// published tarball. Force offline mode here so sqlx-macros use the bundled
// `.sqlx/` query cache instead of trying to reach a database.
fn main() {
    println!("cargo:rustc-env=SQLX_OFFLINE=true");
}
