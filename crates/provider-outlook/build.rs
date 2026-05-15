fn main() {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let env_path = std::path::Path::new(&manifest_dir).join("../../.env");
        if let Ok(contents) = std::fs::read_to_string(env_path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    if key == "OUTLOOK_CLIENT_ID" {
                        println!("cargo:rustc-env={key}={}", value.trim());
                    }
                }
            }
        }
    }
    println!("cargo:rerun-if-changed=../../.env");
}
