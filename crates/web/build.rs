use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_spa_dist)");

    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return;
    };
    let manifest_dir = PathBuf::from(manifest_dir);
    let dist_dir = manifest_dir.join("../../apps/web/dist");
    let index_html = dist_dir.join("index.html");

    println!("cargo:rerun-if-changed={}", dist_dir.display());
    println!("cargo:rerun-if-changed={}", index_html.display());

    if index_html.is_file() {
        println!("cargo:rustc-cfg=has_spa_dist");
    }
}
